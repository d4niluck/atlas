// RMS reflects microphone volume and stays still for actual silence.
export function signalLevel(samples:Float32Array):number {
 if(!samples.length)return 0;
 let power=0;for(const value of samples)power+=value*value;
 return Math.min(1,Math.sqrt(power/samples.length)*5);
}
