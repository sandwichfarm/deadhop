// QR code as a single SVG path. Built from the module matrix so the page
// never has to inject markup.
import { encode } from 'uqr';

export interface QrShape {
  /** Modules per side, quiet zone included. */
  size: number;
  /** SVG path data covering every dark module. */
  path: string;
}

export function qrShape(text: string): QrShape {
  const { data, size } = encode(text, { ecc: 'M', border: 2 });
  let path = '';
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      if (data[y][x]) path += `M${x} ${y}h1v1h-1z`;
    }
  }
  return { size, path };
}
