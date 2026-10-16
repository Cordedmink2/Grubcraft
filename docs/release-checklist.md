# Release checklist

How to cut a release and what to test before telling friends.

## Cutting a release

1. Make sure `main` is green in CI, including the `mac-app` and `windows-exe` jobs. To rehearse without publishing, run the Release workflow by hand from the Actions tab (Run workflow on `main`): it does everything except create the release.
2. Set `version` under `[workspace.package]` in the root `Cargo.toml` and commit it. The game shows this version and the commit (from `crates/gc_game/build.rs`).
3. Tag the commit and push the tag, which must be `v` plus that version:
   `git tag v0.2.0` then `git push origin v0.2.0`. The workflow refuses a tag that does not match the version.
4. The Release workflow runs the whole of CI on the tagged commit (every test, on macOS, Windows and Linux), then attaches the zips that CI built to a GitHub Release for the tag. It takes about 20 minutes. If any CI job fails, nothing is published.
5. Open the release page. Check the files are there: `Grubcraft-macOS.zip`, `Grubcraft-Windows.zip` and `SHA256SUMS` (the checksum of each zip, for anyone who wants to check a download).
6. Run the tests below. If one fails, delete the release and the tag, fix, and tag again.

Everyone in a LAN game must run the same build. Send all friends the same release.

## What to test

Do this from the downloaded zip on a machine with no repo checkout. Not from `target/`.

### Windows

- The zip holds a `Grubcraft` folder with `grubcraft.exe`, `grubcraft.pdb` and `Read me.txt`.
- SmartScreen shows. More info, then Run anyway, starts the game.
- The firewall question appears when hosting. Allow on Private networks.
- The main menu, a battle with AI, and sound all work.

### Mac

- The zip holds `Grubcraft.app` and `Read me.txt`.
- Right-click, Open, confirm starts the game. Try it on both an Apple Silicon and an Intel Mac if you can.
- The local network question appears. Allow it.
- The main menu, a battle with AI, and sound all work.

### LAN battle (one Mac and one Windows PC, same Wi-Fi)

- Each machine shows the other's game in the Join list.
- Joining by join code works. Joining by address works.
- The lobby shows the same version on both. Start works.
- A short battle ends with no desync message.
- Quit one game mid-battle and start it again. It rejoins.

### Crash reports

- Start the game from a terminal as `grubcraft --crash-test` (on a Mac: `Grubcraft.app/Contents/MacOS/grubcraft --crash-test`). It panics on purpose. The game folder (`%APPDATA%\Grubcraft` on Windows, `~/Library/Application Support/Grubcraft` on a Mac) now holds a `crashes/crash-….txt` naming the build, the GPU and a backtrace with function names, and `logs/grubcraft.log` ends with a `PANIC` line.
