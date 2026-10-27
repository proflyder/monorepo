import { createSocket } from "node:dgram";
import { networkInterfaces } from "node:os";

// Minimal DNS responder for the Alice/Station test setup.
//
// - A queries for DNS_NAMES are answered with DNS_TARGET (this machine).
// - AAAA/other queries for DNS_NAMES return NOERROR/NODATA so the station
//   falls back to IPv4 instead of going to the real Yandex hosts.
// - Everything else is forwarded to the upstream resolvers unchanged.

const BIND = process.env.DNS_BIND ?? "0.0.0.0";
const PORT = Number(process.env.DNS_PORT ?? 53);
const TARGET = process.env.DNS_TARGET ?? "192.0.2.11";
const TTL = Number(process.env.DNS_TTL ?? 30);
const NAMES = (process.env.DNS_NAMES ?? "quasar.yandex.net,uniproxy.alice.yandex.net")
  .split(",")
  .map((name) => name.trim().toLowerCase().replace(/\.$/, ""))
  .filter(Boolean);
const UPSTREAMS = (process.env.DNS_UPSTREAM ?? "192.0.2.1,1.1.1.1")
  .split(",")
  .map((item) => item.trim())
  .filter(Boolean)
  .map((address) => ({ address, port: 53 }));

// Answering the proxy host itself would loop its own upstream connections back
// into this resolver. Local clients (and the Mac's own queries) are forwarded.
const SELF = new Set(["127.0.0.1", "::1", "::ffff:127.0.0.1"]);
for (const entries of Object.values(networkInterfaces())) {
  for (const entry of entries ?? []) {
    if (entry.family === "IPv4") SELF.add(entry.address);
  }
}

const CLIENTS = (process.env.DNS_CLIENTS ?? "")
  .split(",")
  .map((item) => item.trim())
  .filter(Boolean);

function shouldSpoof(client: string): boolean {
  if (CLIENTS.length > 0) return CLIENTS.includes(client);
  return !SELF.has(client);
}

const targetBytes = TARGET.split(".").map(Number);
if (targetBytes.length !== 4 || targetBytes.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) {
  throw new Error(`DNS_TARGET is not an IPv4 address: ${TARGET}`);
}

type Question = { name: string; type: number; class: number; end: number; offset: number };

function parseName(msg: Buffer, start: number): { name: string; next: number } | null {
  const labels: string[] = [];
  let pos = start;
  let next = start;
  let jumped = false;
  let hops = 0;

  while (true) {
    if (pos >= msg.length) return null;
    const length = msg[pos];
    if (length === 0) {
      if (!jumped) next = pos + 1;
      break;
    }
    if ((length & 0xc0) === 0xc0) {
      if (pos + 1 >= msg.length) return null;
      if (!jumped) next = pos + 2;
      jumped = true;
      pos = ((length & 0x3f) << 8) | msg[pos + 1];
      if (++hops > 32) return null;
      continue;
    }
    if (length > 63 || pos + 1 + length > msg.length) return null;
    labels.push(msg.toString("ascii", pos + 1, pos + 1 + length));
    pos += 1 + length;
  }

  return { name: labels.join(".").toLowerCase(), next };
}

function parseQuestion(msg: Buffer): Question | null {
  const name = parseName(msg, 12);
  if (!name) return null;
  const typeOffset = name.next;
  if (typeOffset + 4 > msg.length) return null;
  return {
    name: name.name,
    type: msg.readUInt16BE(typeOffset),
    class: msg.readUInt16BE(typeOffset + 2),
    end: typeOffset + 4,
    offset: 12,
  };
}

function isIntercepted(name: string): boolean {
  return NAMES.some((candidate) => name === candidate || name.endsWith(`.${candidate}`));
}

function header(query: Buffer, answers: number): Buffer {
  const out = Buffer.alloc(12);
  query.copy(out, 0, 0, 12);
  // QR=1, AA=1, RD copied from query, RA=1, RCODE=0.
  out.writeUInt16BE(0x8400 | (query.readUInt16BE(2) & 0x0100) | 0x0080, 2);
  out.writeUInt16BE(1, 4);
  out.writeUInt16BE(answers, 6);
  out.writeUInt16BE(0, 8);
  out.writeUInt16BE(0, 10);
  return out;
}

function answerA(query: Buffer, question: Question): Buffer {
  const rr = Buffer.alloc(16);
  rr.writeUInt16BE(0xc000 | question.offset, 0);
  rr.writeUInt16BE(1, 2);
  rr.writeUInt16BE(1, 4);
  rr.writeUInt32BE(TTL, 6);
  rr.writeUInt16BE(4, 10);
  for (let i = 0; i < 4; i++) rr[12 + i] = targetBytes[i];
  return Buffer.concat([header(query, 1), query.subarray(12, question.end), rr]);
}

function answerEmpty(query: Buffer, question: Question): Buffer {
  return Buffer.concat([header(query, 0), query.subarray(12, question.end)]);
}

function queryUpstream(
  message: Buffer,
  upstream: { address: string; port: number },
): Promise<Buffer | null> {
  return new Promise((resolve) => {
    const socket = createSocket("udp4");
    let settled = false;
    let timer: NodeJS.Timeout;

    const finish = (value: Buffer | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try {
        socket.close();
      } catch {
        // already closed
      }
      resolve(value);
    };

    timer = setTimeout(() => finish(null), 2000);
    socket.once("message", (reply) => finish(reply));
    socket.once("error", () => finish(null));
    socket.send(message, upstream.port, upstream.address, (error) => {
      if (error) finish(null);
    });
  });
}

async function forward(message: Buffer): Promise<Buffer | null> {
  for (const upstream of UPSTREAMS) {
    const reply = await queryUpstream(message, upstream);
    if (reply) return reply;
  }
  return null;
}

const server = createSocket("udp4");

server.on("message", (message, remote) => {
  if (message.length < 12) return;
  const flags = message.readUInt16BE(2);
  if (flags & 0x8000) return; // ignore responses
  if ((flags >> 11) & 0x0f) return; // only standard queries
  if (message.readUInt16BE(4) !== 1) {
    void forward(message).then((reply) => reply && server.send(reply, remote.port, remote.address));
    return;
  }

  const question = parseQuestion(message);
  if (!question) return;

  if (isIntercepted(question.name) && shouldSpoof(remote.address)) {
    const reply =
      question.type === 1 && question.class === 1
        ? answerA(message, question)
        : answerEmpty(message, question);
    server.send(reply, remote.port, remote.address);
    console.log(
      `${new Date().toISOString()} ${remote.address}:${remote.port} ${question.name} ` +
        `type=${question.type} -> ${question.type === 1 ? TARGET : "NODATA"}`,
    );
    return;
  }

  void forward(message).then((reply) => {
    if (reply) {
      server.send(reply, remote.port, remote.address);
    } else {
      const failure = header(message, 0);
      failure.writeUInt16BE(0x8182, 2); // SERVFAIL
      server.send(failure, remote.port, remote.address);
    }
  });
});

server.on("error", (error) => {
  console.error(`${new Date().toISOString()} dns error: ${error.message}`);
});

server.bind(PORT, BIND, () => {
  console.log(`alice-mitm dns on ${BIND}:${PORT}`);
  console.log(`  target:    ${TARGET}`);
  console.log(`  names:     ${NAMES.join(", ")}`);
  console.log(`  spoof:     ${CLIENTS.length > 0 ? CLIENTS.join(", ") : "everyone except this host"}`);
  console.log(`  upstreams: ${UPSTREAMS.map((u) => u.address).join(", ")}`);
});

const shutdown = (signal: string) => {
  console.log(`${signal}: stopping dns`);
  try {
    server.close();
  } finally {
    process.exit(0);
  }
};

process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));
