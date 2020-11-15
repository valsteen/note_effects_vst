# Note Generator VST

This VST plugin was made possible thanks to https://github.com/RustAudio/vst-rs

Just select the channel, pitch and velocity and move the trigger above 50%, and a note will be generated.

If you can modulate those parameters like here in bitwig with a beat LFO, you can easily generate notes.



![](https://media.giphy.com/media/ilpQnk0HSNd5vccNs0/giphy.gif)

Find here a screen recording to get a better idea:

<a href="https://www.youtube.com/watch?v=RkMzIqAKo4I"><img src="https://lh3.googleusercontent.com/pw/ACtC-3edwpMgzjFLWGPo-haiGYtn9Mk4hSDCrOYxb_7Y139Sc6A2ZCvIvzLenzIItKFh1eK1I1KzbYeaRGlGXzym9QNFDGryM80rnzI_8O7KyT_ttwuex_3_oYqgdH85xn5lsP5EU2NnRPQPyMI46-aNzY0y2A=w901-h574-no?authuser=0)](https://www.youtube.com/watch?v=RkMzIqAKo4I" data-canonical-src="https://gyazo.com/eb5c5741b6a9a16c692170a41a49c858.png" width="400"  /></a>


I'll be working on making this plugin easily available to non-developers, in the meantime check https://github.com/RustAudio/vst-rs on how to build VST plugins.

After installing Rust this boils down to:

```
cargo build --color=always --all --all-targets --release
```

and then to make it a plugin bundle on MacOSX, use the  [osx_vst_bundler.sh](https://github.com/RustAudio/vst-rs/blob/master/osx_vst_bundler.sh) shellscript available on https://github.com/RustAudio/vst-rs:

```
osx_vst_bundler.sh NoteGenerator ./target/release/libnote_generator.dylib 
```

You'll get a `NoteGenerator.vst` that you can put in a directory where your DAW finds plugins.
