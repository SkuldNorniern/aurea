# aurea-build

Reads an [Aurea](https://docs.rs/aurea) app's identity from its `Cargo.toml` and hands it to the app's
code: display name, bundle id, icon and the file types it opens.

```toml
[package.metadata.aurea]
name = "My App"
id = "dev.example.myapp"
icon = "brand/myapp"
version = "auto"

[build-dependencies]
aurea-build = "0.1"
```

Call `aurea_build::identity()` from the app's `build.rs`, and `aurea::identity!()` in the app picks it
up. Missing keys fall back to the package: its name for `name` and `id`, its version for `version`.

The crate's docs list every key.

## License

Apache-2.0
