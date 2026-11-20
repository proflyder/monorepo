#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const EXCLUDED = [
  /(^|\/)testdata\//,
  /(^|\/)fixtures\//,
  /(^|\/)tests\/golden\//,
  /(^|\/)third_party\//,
  /(^|\/)vendor\//,
  /(^|\/)generated\//,
  /^resources\//,
  /^artifacts\//,
  /\/ricardo\/microfrontend\/(tensorflow|kissfft)\//,
  /\.lock$/,
  /\.patch$/,
  /(^|\/)gradlew(\.bat)?$/,
];

const BY_NAME = new Map([
  ["BUILD", "python"],
  ["BUILD.bazel", "python"],
  ["MODULE.bazel", "python"],
  ["WORKSPACE", "python"],
  ["WORKSPACE.bazel", "python"],
  [".bazelrc", "hash"],
  [".bazelversion", null],
  ["Makefile", "hash"],
  [".editorconfig", "lineHash"],
  [".gitignore", "lineHash"],
  [".dockerignore", "lineHash"],
  [".gitattributes", "lineHash"],
  [".shellcheckrc", "lineHash"],
  [".npmrc", "lineHash"],
]);

const BY_EXT = new Map([
  ["rs", "rust"],
  ["kt", "kotlin"],
  ["kts", "kotlin"],
  ["java", "kotlin"],
  ["js", "js"],
  ["mjs", "js"],
  ["cjs", "js"],
  ["ts", "js"],
  ["tsx", "js"],
  ["jsx", "js"],
  ["go", "c"],
  ["c", "c"],
  ["h", "c"],
  ["cc", "c"],
  ["cpp", "c"],
  ["hpp", "c"],
  ["proto", "c"],
  ["css", "c"],
  ["py", "python"],
  ["bzl", "python"],
  ["bazel", "python"],
  ["sh", "shell"],
  ["bash", "shell"],
  ["zsh", "shell"],
  ["yml", "yaml"],
  ["yaml", "yaml"],
  ["toml", "toml"],
  ["nft", "hash"],
  ["conf", "hash"],
  ["cfg", "hash"],
  ["ini", "lineHash"],
  ["service", "lineHash"],
  ["timer", "lineHash"],
  ["socket", "lineHash"],
  ["properties", "lineHash"],
  ["env", "lineHash"],
  ["ps1", "powershell"],
  ["psm1", "powershell"],
  ["xml", "xml"],
  ["html", "xml"],
  ["svg", "xml"],
  ["sql", "sql"],
]);

const SHEBANG = [
  [/\b(node|deno|bun)\b/, "js"],
  [/\bpython[0-9.]*\b/, "python"],
  [/\b(bash|sh|zsh|dash)\b/, "shell"],
];

function languageForShebang(text) {
  const first = text.startsWith("#!") ? text.slice(0, text.indexOf("\n") >>> 0) : "";
  return SHEBANG.find(([pattern]) => pattern.test(first))?.[1] ?? null;
}

export function languageFor(file, text = "") {
  if (EXCLUDED.some((pattern) => pattern.test(file))) {
    return null;
  }
  const base = path.posix.basename(file);
  if (!base.includes(".") && !BY_NAME.has(base) && !/^(Dockerfile|Containerfile)/.test(base)) {
    return languageForShebang(text);
  }
  if (BY_NAME.has(base)) {
    return BY_NAME.get(base);
  }
  if (/^(Dockerfile|Containerfile)([.-].*)?$/.test(base) || /\.(Dockerfile|Containerfile)$/.test(base)) {
    return "dockerfile";
  }
  if (/^\.env(\..*)?$/.test(base)) {
    return "lineHash";
  }
  if (/^requirements.*\.txt$/.test(base)) {
    return "lineHash";
  }
  const dot = base.lastIndexOf(".");
  if (dot > 0) {
    return BY_EXT.get(base.slice(dot + 1)) ?? null;
  }
  return null;
}

function isIdent(ch) {
  return ch !== undefined && /[A-Za-z0-9_$]/.test(ch);
}

class Lexer {
  constructor(text) {
    this.text = text;
    this.i = 0;
    this.comments = [];
  }

  at(offset = 0) {
    return this.text[this.i + offset];
  }

  startsWith(token) {
    return this.text.startsWith(token, this.i);
  }

  add(start, end, kind) {
    this.comments.push({ start, end, kind });
  }

  lineComment(kind) {
    const start = this.i;
    const newline = this.text.indexOf("\n", this.i);
    this.i = newline === -1 ? this.text.length : newline;
    this.add(start, this.i, kind);
  }

  blockComment(open, close, nested, kind) {
    const start = this.i;
    this.i += open.length;
    let depth = 1;
    while (this.i < this.text.length && depth > 0) {
      if (nested && this.startsWith(open)) {
        depth += 1;
        this.i += open.length;
      } else if (this.startsWith(close)) {
        depth -= 1;
        this.i += close.length;
      } else {
        this.i += 1;
      }
    }
    this.add(start, this.i, kind);
  }

  quoted(quote, escapes = true) {
    this.i += quote.length;
    while (this.i < this.text.length) {
      if (escapes && this.at() === "\\") {
        this.i += 2;
      } else if (this.startsWith(quote)) {
        this.i += quote.length;
        return;
      } else {
        this.i += 1;
      }
    }
  }
}

class CLexer extends Lexer {
  constructor(text, language) {
    super(text);
    this.language = language;
    this.lastSignificant = "";
  }

  run(closeOnBrace = false) {
    let depth = 0;
    while (this.i < this.text.length) {
      const ch = this.at();
      const next = this.at(1);
      if (ch === "/" && next === "/") {
        const kind = this.text[this.i + 2] === "/" || this.text[this.i + 2] === "!" ? "doc" : "line";
        this.lineComment(kind);
        continue;
      }
      if (ch === "/" && next === "*") {
        const kind = this.text[this.i + 2] === "*" || this.text[this.i + 2] === "!" ? "doc" : "block";
        this.blockComment("/*", "*/", this.language === "rust" || this.language === "kotlin", kind);
        continue;
      }
      if (closeOnBrace && ch === "{") {
        depth += 1;
      } else if (closeOnBrace && ch === "}") {
        if (depth === 0) {
          this.i += 1;
          return;
        }
        depth -= 1;
      }
      if (this.language === "rust" && this.rustLiteral()) {
        this.lastSignificant = "a";
        continue;
      }
      if (ch === '"') {
        if (this.language === "kotlin" && this.startsWith('"""')) {
          this.templateString('"""', false);
        } else if (this.language === "kotlin") {
          this.templateString('"', true);
        } else {
          this.quoted('"');
        }
        this.lastSignificant = "a";
        continue;
      }
      if (ch === "'") {
        if (this.language === "rust") {
          this.rustQuote();
        } else {
          this.quoted("'");
        }
        this.lastSignificant = "a";
        continue;
      }
      if (ch === "`" && this.language === "js") {
        this.templateString("`", true);
        this.lastSignificant = "a";
        continue;
      }
      if (ch === "`" && this.language === "kotlin") {
        this.quoted("`", false);
        this.lastSignificant = "a";
        continue;
      }
      if (ch === "/" && this.language === "js" && this.regexAllowed()) {
        this.regex();
        this.lastSignificant = "a";
        continue;
      }
      if (/\s/.test(ch)) {
        this.i += 1;
        continue;
      }
      if (isIdent(ch)) {
        let end = this.i;
        while (isIdent(this.text[end])) {
          end += 1;
        }
        this.lastSignificant = this.text.slice(this.i, end);
        this.i = end;
        continue;
      }
      this.lastSignificant = ch;
      this.i += 1;
    }
  }

  regexAllowed() {
    const last = this.lastSignificant;
    if (last === "") {
      return true;
    }
    if (/^[A-Za-z0-9_$]+$/.test(last)) {
      return /^(return|typeof|case|do|else|in|of|new|delete|void|throw|yield|await|instanceof)$/.test(last);
    }
    return !/^[)\]}]$/.test(last);
  }

  regex() {
    this.i += 1;
    let inClass = false;
    while (this.i < this.text.length) {
      const ch = this.at();
      if (ch === "\\") {
        this.i += 2;
        continue;
      }
      if (ch === "\n") {
        return;
      }
      if (inClass) {
        inClass = ch !== "]";
      } else if (ch === "[") {
        inClass = true;
      } else if (ch === "/") {
        this.i += 1;
        while (/[a-z]/.test(this.at() ?? "")) {
          this.i += 1;
        }
        return;
      }
      this.i += 1;
    }
  }

  templateString(quote, escapes) {
    this.i += quote.length;
    while (this.i < this.text.length) {
      if (escapes && this.at() === "\\") {
        this.i += 2;
        continue;
      }
      if (this.startsWith(quote)) {
        this.i += quote.length;
        if (quote === '"""') {
          while (this.at() === '"') {
            this.i += 1;
          }
        }
        return;
      }
      if (this.at() === "$" && this.at(1) === "{") {
        this.i += 2;
        const saved = this.lastSignificant;
        this.lastSignificant = "";
        this.run(true);
        this.lastSignificant = saved;
        continue;
      }
      this.i += 1;
    }
  }

  rustLiteral() {
    if (isIdent(this.text[this.i - 1])) {
      return false;
    }
    const match = /^(b|c|br|cr|r)(#*)"/.exec(this.text.slice(this.i, this.i + 260));
    if (!match) {
      if (this.at() === "b" && this.at(1) === "'") {
        this.i += 1;
        this.rustQuote();
        return true;
      }
      return false;
    }
    const raw = match[1].includes("r");
    this.i += match[0].length;
    const close = `"${match[2]}`;
    while (this.i < this.text.length) {
      if (!raw && this.at() === "\\") {
        this.i += 2;
      } else if (this.startsWith(close)) {
        this.i += close.length;
        return true;
      } else {
        this.i += 1;
      }
    }
    return true;
  }

  rustQuote() {
    if (this.at(1) === "\\") {
      this.quoted("'");
      return;
    }
    const codePoint = this.text.codePointAt(this.i + 1);
    const width = codePoint !== undefined && codePoint > 0xffff ? 2 : 1;
    if (this.text[this.i + 1 + width] === "'") {
      this.i += 2 + width;
      return;
    }
    this.i += 1;
  }
}

const PY_STRING = /^([rRbBuUfF]{0,2})('''|"""|'|")/;

class PythonLexer extends Lexer {
  run() {
    let depth = 0;
    let lineStart = true;
    let lineStrings = null;
    const finishLine = () => {
      if (lineStrings && lineStrings.only) {
        this.add(lineStrings.start, lineStrings.end, "docstring");
      }
      lineStrings = null;
      lineStart = true;
    };
    while (this.i < this.text.length) {
      const ch = this.at();
      if (ch === "#") {
        this.lineComment("line");
        continue;
      }
      if (ch === "\\" && this.at(1) === "\n") {
        this.i += 2;
        continue;
      }
      if (ch === "\n") {
        this.i += 1;
        if (depth === 0) {
          finishLine();
        }
        continue;
      }
      if (ch === " " || ch === "\t" || ch === "\r") {
        this.i += 1;
        continue;
      }
      const prefix = isIdent(this.text[this.i - 1]) ? null : PY_STRING.exec(this.text.slice(this.i, this.i + 6));
      if (prefix) {
        const start = this.i;
        this.i += prefix[1].length;
        const raw = /[rR]/.test(prefix[1]);
        this.pyString(prefix[2], raw);
        if (lineStart && depth === 0) {
          lineStrings = { start, end: this.i, only: true };
        } else if (lineStrings && lineStrings.only && depth === 0) {
          lineStrings.end = this.i;
        }
        lineStart = false;
        continue;
      }
      if (lineStrings && depth === 0) {
        lineStrings.only = ch === ";" ? lineStrings.only : false;
      }
      if (ch === "(" || ch === "[" || ch === "{") {
        depth += 1;
      } else if ((ch === ")" || ch === "]" || ch === "}") && depth > 0) {
        depth -= 1;
      }
      if (lineStrings && depth > 0) {
        lineStrings.only = false;
      }
      lineStart = false;
      this.i += 1;
    }
    finishLine();
  }

  pyString(quote, raw) {
    this.i += quote.length;
    while (this.i < this.text.length) {
      if (this.at() === "\\") {
        this.i += 2;
        continue;
      }
      if (this.startsWith(quote)) {
        this.i += quote.length;
        return;
      }
      if (quote.length === 1 && this.at() === "\n") {
        return;
      }
      this.i += 1;
    }
    void raw;
  }
}

const SHELL_WORD_BREAK = new Set([undefined, " ", "\t", "\n", ";", "&", "|", "(", ")", "<", ">"]);

class ShellLexer extends Lexer {
  constructor(text) {
    super(text);
    this.heredocs = [];
  }

  run(until = null) {
    while (this.i < this.text.length) {
      const ch = this.at();
      if (until && ch === until) {
        this.i += 1;
        return;
      }
      if (ch === "\\") {
        this.i += 2;
        continue;
      }
      if (ch === "#" && SHELL_WORD_BREAK.has(this.text[this.i - 1])) {
        this.lineComment("line");
        continue;
      }
      if (ch === "\n") {
        this.i += 1;
        this.consumeHeredocs();
        continue;
      }
      if (ch === "'") {
        this.quoted("'", false);
        continue;
      }
      if (ch === "$" && this.at(1) === "'") {
        this.i += 1;
        this.quoted("'");
        continue;
      }
      if (ch === '"') {
        this.doubleQuoted();
        continue;
      }
      if (ch === "`") {
        this.quoted("`");
        continue;
      }
      if (ch === "$" && this.at(1) === "{") {
        this.skipBalanced("{", "}");
        continue;
      }
      if (ch === "$" && this.at(1) === "(" && this.at(2) === "(") {
        this.skipBalanced("(", ")");
        continue;
      }
      if (ch === "$" && this.at(1) === "(") {
        this.i += 2;
        this.run(")");
        continue;
      }
      if (ch === "(" && this.at(1) === "(" && SHELL_WORD_BREAK.has(this.text[this.i - 1])) {
        this.skipBalanced("(", ")", 0);
        continue;
      }
      if (ch === "<" && this.at(1) === "<" && this.at(2) !== "<") {
        this.heredocStart();
        continue;
      }
      if (ch === "(" && until === ")") {
        this.i += 1;
        this.run(")");
        continue;
      }
      this.i += 1;
    }
  }

  doubleQuoted() {
    this.i += 1;
    while (this.i < this.text.length) {
      const ch = this.at();
      if (ch === "\\") {
        this.i += 2;
      } else if (ch === '"') {
        this.i += 1;
        return;
      } else if (ch === "$" && this.at(1) === "(" && this.at(2) !== "(") {
        this.i += 2;
        this.run(")");
      } else if (ch === "$" && (this.at(1) === "{" || this.at(1) === "(")) {
        this.skipBalanced(this.at(1), this.at(1) === "{" ? "}" : ")");
      } else if (ch === "`") {
        this.quoted("`");
      } else {
        this.i += 1;
      }
    }
  }

  skipBalanced(open, close, skip = 1) {
    this.i += skip;
    let depth = 0;
    while (this.i < this.text.length) {
      const ch = this.at();
      if (ch === "\\") {
        this.i += 2;
        continue;
      }
      if (ch === "'" && open === "{") {
        this.quoted("'", false);
        continue;
      }
      if (ch === '"') {
        this.doubleQuoted();
        continue;
      }
      if (ch === open) {
        depth += 1;
      } else if (ch === close) {
        depth -= 1;
        if (depth === 0) {
          this.i += 1;
          return;
        }
      }
      this.i += 1;
    }
  }

  heredocStart() {
    const match = /^<<(-?)[ \t]*(?:'([^'\n]+)'|"([^"\n]+)"|\\?([A-Za-z_][A-Za-z0-9_]*))/.exec(
      this.text.slice(this.i, this.i + 200),
    );
    if (!match) {
      this.i += 2;
      return;
    }
    this.heredocs.push({ strip: match[1] === "-", tag: match[2] ?? match[3] ?? match[4] });
    this.i += match[0].length;
  }

  consumeHeredocs() {
    while (this.heredocs.length) {
      const { strip, tag } = this.heredocs.shift();
      while (this.i < this.text.length) {
        const end = this.text.indexOf("\n", this.i);
        const lineEnd = end === -1 ? this.text.length : end;
        let line = this.text.slice(this.i, lineEnd);
        if (strip) {
          line = line.replace(/^\t+/, "");
        }
        this.i = end === -1 ? this.text.length : end + 1;
        if (line === tag) {
          break;
        }
      }
    }
  }
}

class HashLexer extends Lexer {
  constructor(text, options) {
    super(text);
    this.options = options;
  }

  run() {
    while (this.i < this.text.length) {
      const ch = this.at();
      const lineStart = this.isLineStart();
      if (this.options.lineOnly) {
        if (lineStart && this.options.markers.includes(ch)) {
          this.lineComment("line");
          continue;
        }
        this.skipLine();
        continue;
      }
      if (ch === "#" && (lineStart || /\s/.test(this.text[this.i - 1] ?? "\n") || this.options.anywhere)) {
        this.lineComment("line");
        continue;
      }
      if (this.options.toml && (this.startsWith('"""') || this.startsWith("'''"))) {
        this.quoted(this.text.slice(this.i, this.i + 3), this.at() === '"');
        continue;
      }
      if (ch === '"') {
        this.quoted('"');
        continue;
      }
      if (ch === "'" && this.options.singleQuotes) {
        this.quoted("'", false);
        continue;
      }
      if (ch === "\\" && this.options.backslash) {
        this.i += 2;
        continue;
      }
      this.i += 1;
    }
  }

  isLineStart() {
    let j = this.i - 1;
    while (j >= 0 && (this.text[j] === " " || this.text[j] === "\t")) {
      j -= 1;
    }
    return j < 0 || this.text[j] === "\n";
  }

  skipLine() {
    const newline = this.text.indexOf("\n", this.i);
    this.i = newline === -1 ? this.text.length : newline + 1;
  }
}

class DockerLexer extends Lexer {
  run() {
    let shell = "";
    let shellStart = 0;
    const lines = this.text.split("\n");
    let offset = 0;
    let continued = false;
    for (const line of lines) {
      if (/^\s*#/.test(line)) {
        const start = offset + line.indexOf("#");
        this.add(start, offset + line.length, "line");
      } else if (continued || /^\s*RUN\b/i.test(line)) {
        if (!continued) {
          shell = "";
          shellStart = offset;
        }
        shell += `${line}\n`;
        continued = /\\\s*$/.test(line);
        if (!continued) {
          this.shellComments(shell, shellStart);
        }
      } else {
        continued = /\\\s*$/.test(line);
      }
      offset += line.length + 1;
    }
  }

  shellComments(shell, base) {
    const masked = shell.replace(/^(\s*)RUN\b/i, (match) => " ".repeat(match.length));
    const lexer = new ShellLexer(masked);
    lexer.run();
    for (const comment of lexer.comments) {
      this.add(base + comment.start, base + comment.end, comment.kind);
    }
  }
}

const YAML_EMBEDDED = new Map([
  ["run", "shell"],
  ["script", "shell"],
]);

class YamlLexer extends Lexer {
  run() {
    const lines = this.text.split("\n");
    let offset = 0;
    let block = null;
    let quote = null;
    for (let index = 0; index < lines.length; index += 1) {
      const line = lines[index];
      const indent = line.match(/^ */)[0].length;
      if (block) {
        if (line.trim() === "" || indent > block.parent) {
          if (block.indent === null && line.trim() !== "") {
            block.indent = indent;
          }
          block.lines.push({ line, offset });
          offset += line.length + 1;
          continue;
        }
        this.embedded(block);
        block = null;
      }
      const result = this.scanLine(line, offset, quote);
      quote = result.quote;
      if (result.blockScalar && !quote) {
        block = { parent: indent, indent: null, key: result.key, lines: [] };
      }
      offset += line.length + 1;
    }
    if (block) {
      this.embedded(block);
    }
  }

  embedded(block) {
    const language = YAML_EMBEDDED.get(block.key) ?? languageForKey(block.key);
    if (!language || !block.lines.length) {
      return;
    }
    const indent = block.indent ?? 0;
    const base = block.lines[0].offset;
    const body = block.lines
      .map(({ line }) => " ".repeat(Math.min(indent, line.length)) + line.slice(Math.min(indent, line.length)))
      .join("\n");
    for (const comment of findCommentsIn(language, body)) {
      this.add(base + comment.start, base + comment.end, comment.kind);
    }
  }

  scanLine(line, offset, openQuote) {
    let i = 0;
    let quote = openQuote;
    let scalarStart = true;
    let key = null;
    while (i < line.length) {
      const ch = line[i];
      if (quote) {
        if (quote === '"' && ch === "\\") {
          i += 2;
          continue;
        }
        if (ch === quote) {
          if (quote === "'" && line[i + 1] === "'") {
            i += 2;
            continue;
          }
          quote = null;
          scalarStart = false;
        }
        i += 1;
        continue;
      }
      if (line.startsWith("{{/*", i) || line.startsWith("{{- /*", i)) {
        const end = line.indexOf("*/", i);
        const close = end === -1 ? line.length : line.indexOf("}}", end);
        const stop = close === -1 ? line.length : close + 2;
        this.add(offset + i, offset + stop, "block");
        i = stop;
        continue;
      }
      if (ch === "#" && (i === 0 || line[i - 1] === " " || line[i - 1] === "\t")) {
        this.add(offset + i, offset + line.length, "line");
        return { quote: null, blockScalar: false };
      }
      if ((ch === "'" || ch === '"') && scalarStart) {
        quote = ch;
        i += 1;
        continue;
      }
      if (ch === ":" && (line[i + 1] === " " || i + 1 === line.length)) {
        const keyMatch = /^\s*(?:-\s+)?["']?([^"':#]+?)["']?\s*$/.exec(line.slice(0, i));
        key = keyMatch ? keyMatch[1].trim() : key;
        scalarStart = true;
        i += 1;
        continue;
      }
      if (ch === "-" && (line[i + 1] === " " || i + 1 === line.length) && /^\s*$/.test(line.slice(0, i))) {
        scalarStart = true;
        i += 1;
        continue;
      }
      if (ch === "[" || ch === "{" || ch === ",") {
        scalarStart = true;
        i += 1;
        continue;
      }
      if (ch === " " || ch === "\t") {
        i += 1;
        continue;
      }
      if ((ch === "|" || ch === ">") && scalarStart && /^[|>][-+0-9]*\s*(#.*)?$/.test(line.slice(i))) {
        const rest = line.slice(i);
        const hash = rest.indexOf("#");
        if (hash !== -1) {
          this.add(offset + i + hash, offset + line.length, "line");
        }
        return { quote: null, blockScalar: true, key };
      }
      scalarStart = false;
      i += 1;
    }
    return { quote, blockScalar: false, key };
  }
}

function languageForKey(key) {
  if (!key || !key.includes(".")) {
    return null;
  }
  const language = languageFor(key);
  return language === "yaml" ? null : language;
}

class XmlLexer extends Lexer {
  run() {
    while (this.i < this.text.length) {
      if (this.startsWith("<![CDATA[")) {
        const end = this.text.indexOf("]]>", this.i);
        this.i = end === -1 ? this.text.length : end + 3;
      } else if (this.startsWith("<!--")) {
        this.blockComment("<!--", "-->", false, "block");
      } else if (this.at() === "<") {
        this.tag();
      } else {
        this.i += 1;
      }
    }
  }

  tag() {
    while (this.i < this.text.length && this.at() !== ">") {
      if (this.at() === '"' || this.at() === "'") {
        this.quoted(this.at(), false);
      } else {
        this.i += 1;
      }
    }
    this.i += 1;
  }
}

class SqlLexer extends Lexer {
  run() {
    while (this.i < this.text.length) {
      if (this.startsWith("--")) {
        this.lineComment("line");
      } else if (this.startsWith("/*")) {
        this.blockComment("/*", "*/", false, "block");
      } else if (this.at() === "'" || this.at() === '"') {
        this.quoted(this.at(), false);
      } else {
        this.i += 1;
      }
    }
  }
}

class PowerShellLexer extends Lexer {
  run() {
    while (this.i < this.text.length) {
      if (this.startsWith("<#")) {
        this.blockComment("<#", "#>", false, "block");
      } else if (this.at() === "#") {
        this.lineComment("line");
      } else if (this.startsWith('@"') || this.startsWith("@'")) {
        const close = `${this.at(1)}@`;
        const end = this.text.indexOf(`\n${close}`, this.i);
        this.i = end === -1 ? this.text.length : end + 3;
      } else if (this.at() === '"') {
        this.i += 1;
        while (this.i < this.text.length && this.at() !== '"') {
          this.i += this.at() === "`" ? 2 : 1;
        }
        this.i += 1;
      } else if (this.at() === "'") {
        this.quoted("'", false);
      } else {
        this.i += 1;
      }
    }
  }
}

export function findCommentsIn(language, text) {
  let lexer;
  switch (language) {
    case "rust":
    case "kotlin":
    case "js":
    case "c":
      lexer = new CLexer(text, language);
      break;
    case "python":
      lexer = new PythonLexer(text);
      break;
    case "shell":
      lexer = new ShellLexer(text);
      break;
    case "yaml":
      lexer = new YamlLexer(text);
      break;
    case "toml":
      lexer = new HashLexer(text, { toml: true, singleQuotes: true });
      break;
    case "hash":
      lexer = new HashLexer(text, { singleQuotes: false, backslash: true });
      break;
    case "lineHash":
      lexer = new HashLexer(text, { lineOnly: true, markers: ["#", ";"] });
      break;
    case "dockerfile":
      lexer = new DockerLexer(text);
      break;
    case "xml":
      lexer = new XmlLexer(text);
      break;
    case "sql":
      lexer = new SqlLexer(text);
      break;
    case "powershell":
      lexer = new PowerShellLexer(text);
      break;
    default:
      return [];
  }
  lexer.run();
  return lexer.comments.sort((a, b) => a.start - b.start);
}

export const ALLOWED = [
  { name: "shebang", test: (c) => c.start === 0 && c.text.startsWith("#!") },
  { name: "mise task header", test: (c, file) => /(^|\/)mise-tasks\//.test(file) && /^#(MISE|USAGE)\b/.test(c.text) },
  { name: "shellcheck directive", test: (c) => /^#\s*shellcheck\s+(disable|source|shell)=/.test(c.text) },
  { name: "dockerfile parser directive", test: (c) => c.start === 0 && /^#\s*(syntax|escape|check)=/.test(c.text) },
  { name: "cloud-init header", test: (c) => c.start === 0 && c.text === "#cloud-config" },
  { name: "powershell requires", test: (c) => c.language === "powershell" && /^#Requires\s/i.test(c.text) },
  { name: "rust unsafe justification", test: (c) => c.language === "rust" && /^\/\/ SAFETY: [^\n]+$/.test(c.text) },
];

function lineOf(text, offset) {
  let line = 1;
  for (let i = 0; i < offset; i += 1) {
    if (text.charCodeAt(i) === 10) {
      line += 1;
    }
  }
  return line;
}

export function findComments(file, text, language = languageFor(file, text)) {
  if (!language) {
    return [];
  }
  return findCommentsIn(language, text)
    .map((comment) => ({ ...comment, language, text: text.slice(comment.start, comment.end) }))
    .filter((comment) => !ALLOWED.some((rule) => rule.test(comment, file)))
    .map((comment) => ({ ...comment, file, line: lineOf(text, comment.start) }));
}

export function joinSafetyComments(text) {
  return text.replace(
    /^([ \t]*)\/\/ SAFETY: ([^\n]*)((?:\n[ \t]*\/\/(?![\/!])[^\n]*)+)/gm,
    (match, indent, first, rest) => {
      const tail = rest
        .split("\n")
        .map((line) => line.replace(/^[ \t]*\/\/ ?/, "").trim())
        .filter(Boolean);
      return `${indent}// SAFETY: ${[first.trim(), ...tail].join(" ")}`;
    },
  );
}

function lineBounds(text, offset) {
  const start = text.lastIndexOf("\n", offset - 1) + 1;
  const newline = text.indexOf("\n", offset);
  return { start, end: newline === -1 ? text.length : newline + 1 };
}

function previousLine(text, offset) {
  if (offset === 0) {
    return null;
  }
  return lineBounds(text, offset - 1);
}

function nextLine(text, offset) {
  if (offset >= text.length) {
    return null;
  }
  return lineBounds(text, offset);
}

function isBlank(text, line) {
  return line !== null && /^[ \t\r]*\n?$/.test(text.slice(line.start, line.end));
}

const OPENERS = /[{([]\s*$/;
const CLOSERS = /^\s*[})\]]/;

function removeWholeLines(text, start, end) {
  let from = start;
  let to = end;
  const before = previousLine(text, from);
  const after = nextLine(text, to);
  const beforeText = before ? text.slice(before.start, before.end) : "";
  const afterText = after ? text.slice(after.start, after.end) : "";
  if (isBlank(text, after) && (before === null || isBlank(text, before) || OPENERS.test(beforeText.trimEnd()))) {
    to = after.end;
  } else if (isBlank(text, before) && (after === null || CLOSERS.test(afterText))) {
    from = before.start;
  }
  return text.slice(0, from) + text.slice(to);
}

export function stripComments(text, comments) {
  let result = text;
  for (const comment of [...comments].sort((a, b) => b.start - a.start)) {
    let start = comment.start;
    let end = comment.end;
    const line = lineBounds(result, start);
    const endLine = lineBounds(result, Math.max(end - 1, start));
    const endLineEnd = result[endLine.end - 1] === "\n" ? endLine.end - 1 : endLine.end;
    const before = result.slice(line.start, start);
    const after = result.slice(end, endLineEnd);
    if (/^\s*$/.test(before) && /^\s*$/.test(after)) {
      result = removeWholeLines(result, line.start, endLine.end);
      continue;
    }
    if (/^\s*$/.test(after)) {
      while (start > line.start && /[ \t]/.test(result[start - 1])) {
        start -= 1;
      }
      end = endLineEnd;
    } else if (/[ \t(\[{]$/.test(before) && /^[ \t]/.test(after)) {
      while (/[ \t]/.test(result[end] ?? "")) {
        end += 1;
      }
    }
    result = result.slice(0, start) + result.slice(end);
  }
  return result;
}

function trackedFiles(root, args) {
  if (args.length) {
    return args;
  }
  return execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8" })
    .split("\0")
    .filter(Boolean);
}

function stagedFiles(root) {
  return execFileSync("git", ["diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z"], {
    cwd: root,
    encoding: "utf8",
  })
    .split("\0")
    .filter(Boolean);
}

function readFile(root, file, staged) {
  if (staged) {
    return execFileSync("git", ["show", `:${file}`], { cwd: root, encoding: "utf8", maxBuffer: 1 << 28 });
  }
  const full = path.join(root, file);
  if (!fs.existsSync(full) || !fs.statSync(full).isFile()) {
    return null;
  }
  return fs.readFileSync(full, "utf8");
}

function main(argv) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
  const flags = new Set(argv.filter((arg) => arg.startsWith("--")));
  const positional = argv.filter((arg) => !arg.startsWith("--"));
  const staged = flags.has("--staged");
  const files = staged ? stagedFiles(root) : trackedFiles(root, positional);
  const stats = new Map();
  let total = 0;
  for (const file of files) {
    const text = readFile(root, file, staged);
    if (text === null || text.includes("\0")) {
      continue;
    }
    const language = languageFor(file, text);
    if (!language) {
      continue;
    }
    const comments = findComments(file, text, language);
    if (!comments.length) {
      continue;
    }
    total += comments.length;
    const key = path.posix.extname(file).slice(1) || path.posix.basename(file);
    stats.set(key, (stats.get(key) ?? 0) + comments.length);
    if (flags.has("--fix")) {
      const joined = language === "rust" ? joinSafetyComments(text) : text;
      fs.writeFileSync(path.join(root, file), stripComments(joined, findComments(file, joined, language)));
    } else if (!flags.has("--stats")) {
      for (const comment of comments) {
        const first = comment.text.split("\n")[0].slice(0, 100);
        process.stdout.write(`${file}:${comment.line}: ${first}\n`);
      }
    }
  }
  if (flags.has("--stats") || flags.has("--fix")) {
    for (const [key, count] of [...stats].sort((a, b) => b[1] - a[1])) {
      process.stdout.write(`${key}\t${count}\n`);
    }
    process.stdout.write(`total\t${total}\n`);
    return 0;
  }
  if (total) {
    process.stderr.write(
      `\n${total} comment(s) found. Code carries no comments; move explanations to Markdown ` +
        "(see AGENTS.md, docs/TESTING.md).\n",
    );
    return 1;
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
