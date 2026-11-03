import {test} from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';import vm from 'node:vm';
test('AudioWorklet discards standby audio, flushes a partial tail, and stops on cancel',()=>{
 let Processor:any;const messages:any[]=[];
 class Base{port={onmessage:(_m:any)=>{},postMessage:(m:any)=>messages.push(m)}}
 vm.runInNewContext(readFileSync(new URL('../public/audio-worklet.js',import.meta.url),'utf8'),{AudioWorkletProcessor:Base,sampleRate:48000,registerProcessor:(_name:string,p:any)=>{Processor=p},Float32Array});
 const p=new Processor();const input=[[new Float32Array(128).fill(.5)]];const output=[[new Float32Array(128)]];
 p.process(input,output);assert.equal(messages.filter(m=>m.type==='audio').length,0);
 p.port.onmessage({data:{type:'start',id:'a'}});for(let i=0;i<9;i++)p.process(input,output);
 p.port.onmessage({data:{type:'end',id:'a'}});const frames=messages.filter(m=>m.type==='audio');assert.equal(frames.length,2);assert.equal(frames[0].samples.length,960);assert.equal(frames[1].samples.length,192);assert.equal(frames[1].sequence,1);assert.equal(messages.at(-1).last_seq,1);
 p.port.onmessage({data:{type:'start',id:'b'}});p.process(input,output);p.port.onmessage({data:{type:'cancel'}});p.port.onmessage({data:{type:'end',id:'b'}});assert.equal(messages.filter(m=>m.type==='audio').length,2);
});
