# Viewer mode feasibility (2026-11-03)

Question: can the recompiled game itself be used as the viewer, instead of porting its logic to Rust?

## Findings

- The game runs on this PC under Proton, started from Quiver through `run-windows.sh`. The installed build is v1.0.7. The repo's latest commit has the same date as that release, which suggests but does not prove they match.
- Live memory can be read from Linux. The game is a normal process, `kernel.yama.ptrace_scope` is 0, and guest addresses sit at a fixed offset. It was `0x10000000` in this run; find it by checking known constants such as `0x1ED790`.
- `sub_...` names and addresses from the source match live data. Examples: character IDs at `0x484C49` (stride `0x68`), bone matrices at `0x4785D0` (stride `0x5C0` per fighter), XPR models loaded as a whole.
- Reading memory does not need a rebuild or any change to the game.
- A rebuilt game needs MSVC and the Windows SDK, per the project's docs. This PC has `clang-cl`; `lld-link` and a Windows SDK were not found. Generated code does not include `windows.h`, but the runtime around it targets Win32 and D3D11.
- The game exposes debug variables such as `DOA3_WATCHVA`. They appear to watch guest memory; not tested.

## Options

| Option | Needs | Result |
| --- | --- | --- |
| Live memory as a reference for the Rust viewer | Nothing extra | Rust stays the viewer, with measured answers |
| Viewer mode inside the recompiled game | Windows toolchain, boot and match setup, UI | Exact visuals and physics, Windows build under Proton |
| Refactor the generated code | Very large effort | Not recommended |

## Conclusion

Use live memory as a reference and keep the Rust viewer. In one session it replaced guesswork on joints with measurements, and corrected wrong assumptions. A viewer mode looks possible but much more expensive, and has not been tried. Revisit it only if some system, such as physics, proves too hard to port.

## How to reproduce

1. Start a match in the game.
2. Find the process running `DOA3.exe` and open `/proc/<pid>/mem` read-only.
3. Add the offset to guest addresses.
4. Read bone matrices and the model's vertex buffer from the same frame. Retry until the matrices match before and after the read.
5. Compare against the XPR file. Changed vertices are the ones the game rewrites.
