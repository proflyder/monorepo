export class DispatchError extends Error {
  constructor(code, speech, { httpStatus = 200, cause } = {}) {
    super(speech, cause === undefined ? undefined : { cause });
    this.name = "DispatchError";
    this.code = code;
    this.speech = clampSpeech(speech);
    this.httpStatus = httpStatus;
  }
}

export function clampSpeech(text) {
  const value = String(text ?? "").replace(/\s+/gu, " ").trim();
  if (value.length <= 160) return value;
  return `${value.slice(0, 159).trimEnd()}…`;
}

export function normalizeError(error) {
  if (error instanceof DispatchError) return error;
  return new DispatchError("internal_error", "Внутренняя ошибка диспетчера.", {
    httpStatus: 500,
    cause: error,
  });
}
