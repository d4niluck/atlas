// RMS reflects microphone volume and stays still for actual silence.
export function signalLevel(samples:Float32Array):number {
 if(!samples.length)return 0;
 let power=0;for(const value of samples)power+=value*value;
 return Math.min(1,Math.sqrt(power/samples.length)*5);
}

// A short noise spike must not arm silence detection; keep recording before speech.
export function createSilenceDetector(sampleRate:number){
 const frameSamples=Math.max(1,Math.round(sampleRate/100));
 let speechSamples=0,silentSamples=0,started=false,stopped=false,power=0,count=0;
 return {feed(samples:Float32Array):boolean{
  if(stopped||!samples.length)return false;
  for(const value of samples){
   power+=value*value;count++;
   if(count<frameSamples)continue;
   const rms=Math.sqrt(power/count);power=0;count=0;
   if(!started){
    speechSamples=rms>=0.004?speechSamples+frameSamples:0;
    if(speechSamples>=sampleRate*0.15){started=true;silentSamples=0}
   }else{
    if(rms>=0.0025)silentSamples=0;else silentSamples+=frameSamples;
    if(silentSamples>=sampleRate*2){stopped=true;return true}
   }
  }
  return false;
 }};
}
