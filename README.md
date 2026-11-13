# ToonShift Plugins

The public catalog for [ToonShift](https://github.com/ThuGie/ToonShift), a Rust interface for the Eneco Toon 2.

| Plugin | Source |
| --- | --- |
| Weather 1.2.0 | Current conditions and a five-day forecast. Tap the tile and choose **Change location**. The package includes executable forecast and search decoding. |
| Outdoor air quality and Outdoor wind | Outdoor European AQI and wind speed for the location selected in each plugin. |
| Room sensor | Compensated room temperature from the Toon thermostat service. |
| Electricity in 24 hours, Gas in 24 hours, Grid export, Water in 24 hours, Solar in 24 hours | Recorded meter history for the last 24 hours. Each tile requires the corresponding firmware archive. Tap a tile to open its chart on compatible ToonShift builds. |
| Humidity 1.0.0 | Compensated room humidity from the firmware's thermostat feed. |
| Electricity and Gas 1.0.0 | Live readings from the firmware's meter adapter. They show **No meter data** until a compatible connected meter reports. |
| Solar | External collector. Publish a UTF-8 line such as `482 W` to `/mnt/data/toon-rust/values/toonshift-solar.txt`. |

## Publish a catalog

Publish `catalog.sig` beside `catalog.json` in the same commit. The signature is 128 hexadecimal digits encoding a detached Ed25519 signature over the exact LF-terminated catalog bytes. With OpenSSL, run `openssl pkeyutl -sign -rawin -inkey /path/to/private.pem -in catalog.json -out catalog.sig.bin` and `xxd -p -c 64 catalog.sig.bin > catalog.sig`. Keep the private key outside Git. Distribute the raw Ed25519 public key independently so users can provision it before adding your source.

Put a version 1 `catalog.json` at the root of a public GitHub repository. Each entry links to a separate JSON package under `plugins/` and records the SHA-256 of its exact Git blob. Match the ID and release in both files, increment the release when the package changes, and update the hash. Use globally unique IDs, such as `yourname-room-humidity`. Store **Refresh official** fetches the index. **Install** or **Update** downloads and verifies the selected package.

Use ToonShift's `scripts/sync-plugin-repo.ps1` with the repository name and full commit SHA. Reopen Store to see the widgets. See the [plugin documentation](https://github.com/ThuGie/ToonShift/blob/main/docs/plugins.md).

## Planned integrations

These are ToonShift manifests. ReToon-apps uses a separate JavaScript runtime. Its PowerFlow and Weather apps informed the meter and weather widgets here.

Weather data is provided by [Open-Meteo](https://open-meteo.com/).

## Executable example

Room summary ships as a WebAssembly plugin. Its code is in `source/room-summary.wat`. Compile with `wat2wasm`, hex-encode the resulting module into the package's `wasm` field, and regenerate the package hash in `catalog.json`. The host ABI is documented in ToonShift's plugin guide.
# Building Weather

Weather 1.2 ships a Rust WebAssembly module. Its source in `source/weather` owns Open-Meteo forecast and location decoding and English and Dutch condition names. It needs a launcher with the executable HTTPS feed ABI.

Run `rustup target add wasm32-unknown-unknown`, then build with `RUSTFLAGS="-C link-arg=-zstack-size=65536" cargo build --manifest-path source/weather/Cargo.toml --target wasm32-unknown-unknown --locked --release` in a POSIX shell. Encode the resulting `toonshift_weather.wasm` as lowercase hexadecimal in the package's `wasm` field. Update the catalog SHA-256 using the final package bytes with LF line endings.
