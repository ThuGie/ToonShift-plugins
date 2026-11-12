# ToonShift Plugins

The public catalog for [ToonShift](https://github.com/ThuGie/ToonShift), a Rust interface for the Eneco Toon 2.

This starter catalog contains display widgets for electricity, gas, solar, humidity, and weather. Each requires a collector to write a formatted value. For example, atomically publish a UTF-8 line such as `482 W` to `/mnt/data/toon-rust/values/toonshift-power.txt` on the device. The widgets show **Unavailable** until data arrives.

## Publish a catalog

Put a version 1 `catalog.json` at the root of a public GitHub repository. Use globally unique plugin IDs, such as `yourname-room-humidity`. Each entry needs `id`, `title`, `description`, and `source`. Supported sources are `clock`, `boiler`, `next_target`, `note` with a `text` field, and `file`. Catalogs describe widgets using data. The app never executes code from a catalog.

Use ToonShift's `scripts/sync-plugin-repo.ps1` with the repository name and full commit SHA. Reopen Store to see the widgets. See the [plugin documentation](https://github.com/ThuGie/ToonShift/blob/main/docs/plugins.md).

## Planned integrations

Independent native integrations inspired by common ToonStore use cases include Thermostat+, P1 energy, solar inverters, weather and radar, Z-Wave and smart plugs, Home Assistant, and air quality. Each integration needs verified hardware behavior or a recorded fixture, and any direct port needs a license review.
