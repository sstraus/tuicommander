// Chrome captures PNG; canvas chooses the per-call output encoding.
export function captureOptions(args = {}) {
  const format = args.format ?? 'webp';
  const quality = args.quality ?? 25;
  if (!['webp','png'].includes(format)) throw new Error('invalid_format: Choose webp or png.');
  if (!Number.isInteger(quality) || quality < 1 || quality > 100) throw new Error('invalid_quality: Use an integer from 1 to 100.');
  if (args.max_long_edge !== undefined && (!Number.isSafeInteger(args.max_long_edge) || args.max_long_edge < 1)) throw new Error('invalid_max_long_edge: Use a positive integer.');
  return {format,quality,max_long_edge:args.max_long_edge};
}

export async function encodeCapture(bitmap, args = {}, crop) {
  const {format,quality,max_long_edge} = captureOptions(args);
  const {x = 0,y = 0,width = bitmap.width,height = bitmap.height} = crop ?? {};
  const scale = Math.min(1, (max_long_edge ?? Math.max(width,height)) / Math.max(width,height));
  const canvas = new OffscreenCanvas(Math.max(1,Math.round(width * scale)),Math.max(1,Math.round(height * scale)));
  try { canvas.getContext('2d').drawImage(bitmap,x,y,width,height,0,0,canvas.width,canvas.height); }
  finally { bitmap.close(); }
  const type = `image/${format}`;
  const blob = await canvas.convertToBlob({type,...(format === 'webp' ? {quality:quality / 100} : {})});
  if (blob.type !== type) throw new Error('unsupported_encoding: Browser did not encode the requested format.');
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = ''; for (const byte of bytes) binary += String.fromCharCode(byte);
  return {image:`data:${type};base64,${btoa(binary)}`,format,quality:format === 'webp' ? quality : null,bytes:blob.size,width:canvas.width,height:canvas.height};
}
