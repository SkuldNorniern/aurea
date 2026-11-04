# aurea-animation

Time-based animations and eased values for Aurea, with no dependency on a window.

```rust
use aurea_animation::{Animation, EaseMode};
use std::time::Duration;

let mut fade = Animation::new(Duration::from_secs(2)).ease(EaseMode::OutCubic);
// Each frame, with the time since the last one:
let progress = fade.tick(Duration::from_millis(16));
assert!(progress.is_some_and(|t| t > 0.0 && t < 1.0));
```

- `Animation`: a timed run that gives eased progress.
- `Tweens`: values that ease towards targets, one per key, for hover highlights and panels sliding in.

In an Aurea app it is `aurea::animation`.

## License

Apache-2.0
