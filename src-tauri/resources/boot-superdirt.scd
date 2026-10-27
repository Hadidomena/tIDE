// tIDE — SuperDirt boot script (template; __TIDE_MARKER__ is substituted at launch).
//
//! The readiness marker file is written explicitly (File write + close) because sclang's
//! exit path is not guaranteed to flush buffered stdout — the M2 step-0 spike lost every
//! marker printed after the early ones. Rust watches the marker plus the OSC port:
//! a bound port alone proved unreliable there too (57120 was up while SuperDirt.new threw).
//
// __TIDE_SAMPLES__ is the Dirt-Samples root (owner manual test, step 3): SuperDirt's
// `start` does NOT load samples — without `loadSoundFiles` every `s "bd"` fails with
// "instrument not found".
//
// NOTE: prettier must never format this file (.prettierignore → *.scd): its fallback
// parser for unknown extensions is markdown, which mangles this code.
(
var marker = "__TIDE_MARKER__";
var samples = "__TIDE_SAMPLES__";
var writeMarker = { |text|
	var f = File(marker, "w");
	f.write(text ++ "\n");
	f.close;
};
// SuperDirt needs no audio input, and an input device whose sample rate differs from the
// output (common with Bluetooth/USB mics: 16 kHz in, 44.1 kHz out) makes scsynth refuse
// to boot. Disabling input buses makes startup deterministic on every device setup.
s.options.numInputBusChannels = 0;
// Canonical SuperDirt server sizing: the default 1024 buffers cannot hold the whole
// Dirt-Samples pack (~thousands of files) — without these, every sample past the limit
// fails with "No more buffer numbers" and prints a stack trace (a 100k-line flood that
// locks up the UI).
s.options.numBuffers = 1024 * 256;
s.options.memSize = 8192 * 32;
s.options.maxNodes = 1024 * 32;
"tIDE: booting SuperCollider server...".postln;
s.waitForBoot(
	{
		"tIDE: server booted -- starting SuperDirt".postln;
		{
			~tideDirt = SuperDirt(2, s);
			if (samples.size > 0) {
				"tIDE: loading samples from ".postln;
				samples.postln;
				~tideDirt.loadSoundFiles(samples ++ "/*");
			} {
				"tIDE: no Dirt-Samples directory configured -- patterns will not sound".postln;
			};
			~tideDirt.start(57120, [0]);
			fork {
				// Let the dirt responders settle before declaring readiness (samples keep
				// loading in the background; the first eval may briefly miss a group).
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
		// Watchdog: scsynth exiting later leaves sclang alive and lying otherwise.
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
