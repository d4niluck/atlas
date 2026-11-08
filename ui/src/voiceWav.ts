export function encodeWav(source:Float32Array,sampleRate:number):Uint8Array{
 const count=Math.floor(source.length*16000/sampleRate),result=new Uint8Array(44+count*2),v=new DataView(result.buffer);
 const write=(p:number,s:string)=>{for(let i=0;i<s.length;i++)v.setUint8(p+i,s.charCodeAt(i))};
 write(0,'RIFF');v.setUint32(4,result.length-8,true);write(8,'WAVE');write(12,'fmt ');v.setUint32(16,16,true);v.setUint16(20,1,true);v.setUint16(22,1,true);v.setUint32(24,16000,true);v.setUint32(28,32000,true);v.setUint16(32,2,true);v.setUint16(34,16,true);write(36,'data');v.setUint32(40,count*2,true);
 for(let i=0;i<count;i++){const x=i*sampleRate/16000,index=Math.floor(x),value=source[index]+(source[Math.min(index+1,source.length-1)]-source[index])*(x-index);v.setInt16(44+i*2,Math.max(-1,Math.min(1,value))*32767,true)}
 return result;
}
