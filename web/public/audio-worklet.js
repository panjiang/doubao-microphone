class PttCapture extends AudioWorkletProcessor {
  constructor(){super();this.active=null;this.sequence=0;this.buffer=new Float32Array(Math.round(sampleRate/50));this.offset=0;this.meter=0;
    this.port.onmessage=({data})=>{
      if(data.type==='start'){this.active=data.id;this.sequence=0;this.offset=0;}
      if((data.type==='end'||data.type==='abort')&&data.id===this.active){if(data.type==='end')this.flush();this.port.postMessage({type:'end',id:this.active,last_seq:this.sequence?this.sequence-1:null});this.active=null;this.offset=0;}
      if(data.type==='cancel'){this.active=null;this.offset=0;}
    };
  }
  flush(){if(this.active&&this.offset){const samples=this.buffer.slice(0,this.offset);this.port.postMessage({type:'audio',id:this.active,sequence:this.sequence++,samples,at:currentTime},[samples.buffer]);this.offset=0;}}
  process(inputs,outputs){
    // Keep a zero-gain output graph alive without playing the microphone locally.
    for(const channel of outputs[0]||[])channel.fill(0);
    const input=inputs[0];if(!input?.length)return true;
    let energy=0;const count=input[0].length;
    for(let i=0;i<count;i++){let s=0;for(const ch of input)s+=ch[i]||0;s/=input.length;energy+=s*s;
      if(this.active){this.buffer[this.offset++]=s;if(this.offset===this.buffer.length)this.flush();}}
    if(++this.meter>=12){this.meter=0;this.port.postMessage({type:'meter',level:Math.sqrt(energy/count)});}return true;
  }
}
registerProcessor('ptt-capture',PttCapture);
