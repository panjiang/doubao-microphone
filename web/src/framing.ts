export const HEADER = 24;
export function uuidBytes(id: string): Uint8Array {
  if(!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)) throw Error('invalid session id');
  return Uint8Array.from(id.replaceAll('-','').match(/../g)!, b=>parseInt(b,16));
}
export function frame(id:string,seq:number,samples:Float32Array):ArrayBuffer {
  if(!Number.isInteger(seq)||seq<0||seq>0xffffffff)throw Error('sequence exhausted');
  const bytes=new ArrayBuffer(HEADER+samples.length*2), v=new DataView(bytes);
  new Uint8Array(bytes).set([80,80,84,49]);new Uint8Array(bytes).set(uuidBytes(id),4);v.setUint32(20,seq,true);
  samples.forEach((s,i)=>{const n=Number.isFinite(s)?Math.max(-1,Math.min(1,s)):0;v.setInt16(HEADER+i*2,Math.round(n*(n<0?32768:32767)),true)});
  return bytes;
}
export function congested(bufferedBytes:number,sampleRate:number):boolean {return bufferedBytes>sampleRate*2*.2;}
