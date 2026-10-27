(
var marker = "__TIDE_MARKER__";
var samples = "__TIDE_SAMPLES__";
var inDevice = "__TIDE_IN_DEVICE__";
var outDevice = "__TIDE_OUT_DEVICE__";
var writeMarker = { |text|
	var f = File(marker, "w");
	f.write(text ++ "\n");
	f.close;
};

var chipSynths = {
	SynthDef(\chip, { |out, freq = 440, sustain = 1, gain = 1, pan = 0, pw = 0.5|
		var env = EnvGen.kr(Env.linen(0.002, sustain.max(0.01), 0.02), doneAction: 2);
		var sig = Pulse.ar(freq, pw.clip(0.05, 0.95)) * env * gain;
		Out.ar(out, Pan2.ar(sig, pan));
	}).add;
	SynthDef(\chipbass, { |out, freq = 220, sustain = 1, gain = 1, pan = 0|
		var env = EnvGen.kr(Env.linen(0.002, sustain.max(0.01), 0.02), doneAction: 2);
		var sig = (LFTri.ar(freq) * 0.8) + (Pulse.ar(freq, 0.5) * 0.15);
		Out.ar(out, Pan2.ar(sig * env * gain, pan));
	}).add;
	SynthDef(\chipled, { |out, freq = 880, sustain = 0.5, gain = 1, pan = 0, pw = 0.125|
		var env = EnvGen.kr(Env.perc(0.001, sustain.max(0.01)), doneAction: 2);
		var sig = Pulse.ar(freq, pw.clip(0.05, 0.95)) * env * gain;
		Out.ar(out, Pan2.ar(sig, pan));
	}).add;
	SynthDef(\chipnoise, { |out, freq = 1000, sustain = 0.25, gain = 1, pan = 0|
		var env = EnvGen.kr(Env.perc(0.001, sustain.max(0.01)), doneAction: 2);
		var sig = BPF.ar(WhiteNoise.ar, freq.clip(80, 12000), 0.4) * 2 * env * gain;
		Out.ar(out, Pan2.ar(sig, pan));
	}).add;
	s.sync;
};
s.options.numInputBusChannels = 0;
if (inDevice.size > 0) {
	s.options.inDevice = inDevice;
	s.options.numInputBusChannels = 2;
};
if (outDevice.size > 0) {
	s.options.outDevice = outDevice;
};
s.options.numBuffers = 1024 * 256;
s.options.memSize = 8192 * 32;
s.options.maxNodes = 1024 * 32;
s.options.numWireBufs = 128;
"tIDE: booting SuperCollider server...".postln;
s.waitForBoot(
	{
		"tIDE: server booted -- starting SuperDirt".postln;
		{
			chipSynths.value;
			~tideDirt = SuperDirt(2, s);
			if (samples.size > 0) {
				"tIDE: loading samples from ".postln;
				samples.postln;
				~tideDirt.loadSoundFiles(samples ++ "/*");
			} {
				"tIDE: no Dirt-Samples directory configured -- patterns will not sound".postln;
			};
			~tideDirt.start(57120, 0 ! 16);
			fork {
				1.wait;
				writeMarker.value("ready");
				"tIDE: engine ready".postln;
			};
		}.try({
			|err|
			writeMarker.value(
				"failed: " ++ err.class.name.asString ++ ": " ++ err.errorString
			);
			"tIDE: engine failed".postln;
			err.errorString.postln;
		});
		fork {
			loop {
				2.wait;
				if (s.serverRunning.not) {
					writeMarker.value("failed: server stopped unexpectedly");
					"tIDE: server stopped".postln;
					0.exit;
				};
			};
		};
	},
	30,
	{
		writeMarker.value("failed: server did not boot within 30 s");
		"tIDE: server boot failed".postln;
		0.exit;
	}
);
)
