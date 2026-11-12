# ToonShift Plugins

The public catalog for [ToonShift](https://github.com/ThuGie/ToonShift), a Rust interface for the Eneco Toon 2.

| Plugin | Source |
| --- | --- |
| Weather 1.0.0 | Current conditions and five-day forecast from Open-Meteo. Tap the installed tile and choose **Change location**. The repo defines supported places, data paths and weather labels. Amsterdam is the initial selection. |
| Humidity 1.0.0 | Compensated room humidity from the firmware's thermostat feed. |
| Electricity and Gas 1.0.0 | Live readings from the firmware's meter adapter. They show **No meter data** until a compatible connected meter reports. |
| Solar | External collector. Publish a UTF-8 line such as `482 W` to `/mnt/data/toon-rust/values/toonshift-solar.txt`. |

## Publish a catalog

Put a version 1 `catalog.json` at the root of a public GitHub repository. Use globally unique plugin IDs, such as `yourname-room-humidity`. Each entry needs `id`, `title`, `description`, and `source`. Give each plugin a `release` and increment it when its configuration changes. Current ToonShift builds support `https_json` and `box_talk` in addition to the original display sources. Catalogs describe widgets using data. The app never executes code from a catalog.

Use ToonShift's `scripts/sync-plugin-repo.ps1` with the repository name and full commit SHA. Reopen Store to see the widgets. See the [plugin documentation](https://github.com/ThuGie/ToonShift/blob/main/docs/plugins.md).

## Planned integrations

Independent native integrations inspired by common ToonStore use cases include Thermostat+, P1 energy history, solar inverters, weather radar, Z-Wave and smart plugs, Home Assistant, and air quality.

Weather data is provided by [Open-Meteo](https://open-meteo.com/).
