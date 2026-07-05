// Fair first-mover selection via Blum-style commit-reveal coin flipping.
//
// Each player draws a random 32-byte nonce, publishes SHA-256(nonce) together
// with their board commitment, and reveals the nonce only after holding the
// opponent's hash. Neither player can bias the result: the nonce is fixed
// (binding) before the opponent's value is known (hiding). The shared coin is
// the XOR-parity of the two nonces' last bytes: 0 → seat 0 fires first,
// 1 → seat 1 fires first. Verification runs in the *verifier's own* browser,
// i.e. the same trust domain as the player it protects.

export type Coin = {
  nonceHex: string;
  commitHex: string;
};

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  }
  return out;
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
  return toHex(new Uint8Array(digest));
}

/** Draw a fresh nonce and its commitment. */
export async function makeCoin(): Promise<Coin> {
  const nonce = new Uint8Array(32);
  crypto.getRandomValues(nonce);
  const nonceHex = toHex(nonce);
  return { nonceHex, commitHex: await sha256Hex(nonce) };
}

/** Check an opponent's reveal against their earlier commitment. */
export async function verifyReveal(nonceHex: string, commitHex: string): Promise<boolean> {
  if (!/^[0-9a-f]{64}$/i.test(nonceHex)) return false;
  return (await sha256Hex(fromHex(nonceHex))).toLowerCase() === commitHex.toLowerCase();
}

/** The shared coin: which SEAT fires first (0 or 1). */
export function firstSeat(myNonceHex: string, theirNonceHex: string): number {
  const mine = fromHex(myNonceHex);
  const theirs = fromHex(theirNonceHex);
  return (mine[mine.length - 1] ^ theirs[theirs.length - 1]) & 1;
}
