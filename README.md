# derive.rs

![demo](https://i.imgur.com/SXYasIX.gif)

Rust reimplementation of [derive](https://github.com/erik/derive).

```
cargo run --release -- \
          --lat=47.2 \
          --lon=6.7 \
          --zoom 8 \
          --width=1200 \
          --height=1200 \
          --stream \
          --frame-rate 50 \
          ~/Downloads/strava/activities \
| ffmpeg -i - -y heatmap.mp4
```
