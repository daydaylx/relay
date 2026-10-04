const likelySecretPatterns = [
  /-----BEGIN (?:OPENSSH |RSA |EC |PGP )?PRIVATE KEY-----/i,
  /\b(?:sk-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{20,}|AKIA[A-Z0-9]{16})\b/,
  /\bBearer\s+[A-Za-z0-9._~+/-]{16,}={0,2}/i,
  /\b(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|passwd|passphrase|secret|private[_-]?key)\b\s*(?:is\s+|[:=]\s*)["']?[^\s"'`,;]{8,}/i,
  /^\s*[A-Za-z0-9_.-]*(?:password|passwd|passphrase|secret|private[_-]?key|api[_-]?key|access[_-]?token|refresh[_-]?token|credential|psk)[A-Za-z0-9_.-]*\s*[:=]\s*\S.{7,}$/im,
  /(?:password|passwd|secret|private[_-]?key|api[_-]?key|access[_-]?token|refresh[_-]?token|credential|psk)\s*[:=]\s*["'][^"']{8,}["']/i,
];

export function containsLikelySecret(text: string): boolean {
  return likelySecretPatterns.some((pattern) => pattern.test(text));
}
