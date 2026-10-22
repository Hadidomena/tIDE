(
var marker = "**TIDE_MARKER**";
var writeMarker = { |text|
var f = File(marker, "w");
f.write(text ++ "\n");
f.close;
};

s.options.numInputBusChannels = 0;
"tIDE: booting SuperCollider server...".postln;
s.waitForBoot(
{
"tIDE: server booted -- starting SuperDirt".postln;
{
~tideDirt = SuperDirt(2, s);
~tideDirt.start(57120, [0]);
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
