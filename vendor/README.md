# Vendored Bevy ecosystem crates

These crates are vendored so we can pin **Bevy 0.19** while upstream packages
still declare older Bevy versions:

| Crate | Upstream | Why vendored |
| --- | --- | --- |
| `iyes_perf_ui` | [XertroV/iyes_perf_ui](https://github.com/XertroV/iyes_perf_ui) (Bevy 0.18) | Text API (`FontSource` / `FontSize`) updated for 0.19 |
| `vleue_navigator` | [vleue/vleue_navigator](https://github.com/vleue/vleue_navigator) v0.15 (Bevy 0.18) | Bevy dep bumped to 0.19; **AI feature only**. May still need glam/polyanya work if you enable `ai`. |

When upstream publishes Bevy 0.19-compatible releases, switch `Cargo.toml` back
to crates.io versions and remove these directories.
