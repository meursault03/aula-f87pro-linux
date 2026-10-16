#!/usr/bin/env python3
"""Native GTK smoke test; run in dbus-run-session + Xvfb, never writes to HID."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
ARTIFACTS = ROOT / "analysis/live"
ARTIFACTS.mkdir(parents=True, exist_ok=True)

MOCK = r'''#!/usr/bin/python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).parent
state = json.loads((root / 'state.json').read_text())
with (root / 'calls.jsonl').open('a') as f:
    f.write(json.dumps(sys.argv[1:]) + '\n')
if sys.argv[1] == 'snapshot':
    print(json.dumps(state))
elif sys.argv[1] == 'apply-state':
    desired = json.loads(pathlib.Path(sys.argv[2]).read_text())
    expected = json.loads(pathlib.Path(sys.argv[3]).read_text())
    if state != expected:
        print('keyboard changed since it was loaded; refresh before applying', file=sys.stderr)
        sys.exit(1)
    backup = pathlib.Path.cwd() / ('aula-f87pro-state-' + str(time.time_ns()) + '.json')
    backup.write_text(json.dumps(state))
    (root / 'state.json').write_text(json.dumps(desired))
    print(json.dumps({'changed': desired != state, 'backup': str(backup), 'state': desired}))
else:
    sys.exit(2)
'''


def main():
    with tempfile.TemporaryDirectory(prefix="f87pro-gui-smoke-") as temp:
        folder = Path(temp)
        shutil.copy2(ROOT / "target/debug/aula-f87pro-gui", folder / "aula-f87pro-gui")
        backend = folder / "aula-f87pro"
        backend.write_text(MOCK)
        backend.chmod(0o755)
        fixture = ROOT / "tests/fixtures"
        original = {"version": 1, "identity": [3, 0, 0, 0, 1, 11],
                    "leds": list((fixture / "f87pro-led-read.bin").read_bytes()),
                    "layers": [list((fixture / "f87pro-matrix-layer0.bin").read_bytes()) for _ in range(4)],
                    "macros": [0] * 4096}
        english = os.environ.get("F87_TEST_ENGLISH") == "1"
        example = "Example" if english else "Exemplo"
        imported_name = "Gaming" if english else "Jogos"
        radio = os.environ.get("F87_TEST_RADIO") == "1"
        if radio:
            cable_rgb = json.loads(json.dumps(original))
            cable_rgb["leds"][10] = 2
            profiles = folder / "data/aula-f87pro/profiles"
            profiles.mkdir(parents=True)
            (profiles / "cable-rgb.json").write_text(json.dumps({"name": "USB RGB" if english else "RGB via cabo", "state": cable_rgb}))
            original["version"] = 2
            original["layers"] = [layer[:504] for layer in original["layers"][:2]]
            original["macros"] = original["macros"][:512]
            original["leds"][14] = 1
        (folder / "state.json").write_text(json.dumps(original))
        env = {**os.environ, "GDK_BACKEND": "x11", "GSK_RENDERER": "cairo", "GTK_A11Y": "none", "GDK_DEBUG": "no-portals", "RUST_BACKTRACE": "1", "XDG_DATA_HOME": str(folder / "data")}
        reduced_motion = os.environ.get("F87_TEST_REDUCED_MOTION") == "1"
        settings = folder / "config/gtk-4.0/settings.ini"
        settings.parent.mkdir(parents=True)
        settings.write_text("[Settings]\ngtk-font-name=DejaVu Sans 10\n" +
                            ("gtk-enable-animations=false\n" if reduced_motion else ""))
        env["XDG_CONFIG_HOME"] = str(folder / "config")
        preference_file = folder / "config/aula-f87pro/settings.json"
        preference_file.parent.mkdir(parents=True)
        preference_file.write_text(json.dumps({"english": english}))
        with (ARTIFACTS / "gui-smoke.log").open("w") as log:
            app = subprocess.Popen([str(folder / "aula-f87pro-gui")], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                window = subprocess.check_output(["xdotool", "search", "--sync", "--onlyvisible", "--name", "AULA F87 Pro"], timeout=10).decode().splitlines()[0]
                def click(x, y):
                    subprocess.run(["xdotool", "mousemove", "--window", window, str(x), str(y), "click", "1"], check=True)
                    time.sleep(.2)
                def key(*keys):
                    subprocess.run(["xdotool", "key", "--clearmodifiers", *keys], check=True)
                    time.sleep(.2)
                def capture(name):
                    destination = os.environ.get("F87_SCREENSHOT_DIR")
                    public = {"gui-lighting-preview.png": ("lighting.png", 960), "gui-keys.png": ("keys.png", 1100), "gui-profiles-final.png": ("profiles.png", 900), "gui-macro-filled.png": ("macros.png", 900), "gui-settings.png": ("settings.png", 1400)}
                    if destination and name in public:
                        filename, height = public[name]
                        subprocess.run(["xdotool", "windowsize", window, "1080", str(height)], check=True)
                        time.sleep(.3)
                        target = Path(destination)
                        target.mkdir(parents=True, exist_ok=True)
                        subprocess.run(["import", "-window", window, str(target / filename)], check=True)
                        subprocess.run(["xdotool", "windowsize", window, "1080", "900"], check=True)
                        time.sleep(.3)
                    subprocess.run(["import", "-window", window, str(ARTIFACTS / name)], check=True)
                def choose_file(title, path):
                    try:
                        dialog = subprocess.check_output(["xdotool", "search", "--sync", "--onlyvisible", "--name", title], timeout=8).decode().splitlines()[-1]
                    except subprocess.TimeoutExpired:
                        subprocess.run(["import", "-window", "root", str(ARTIFACTS / "gui-dialog-failure.png")], check=True)
                        windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", "."]).decode().splitlines()
                        print("Visible windows:", [(w, subprocess.check_output(["xdotool", "getwindowname", w]).decode().strip()) for w in windows], flush=True)
                        raise
                    subprocess.run(["xdotool", "windowfocus", dialog], check=True)
                    time.sleep(.3)
                    if title.startswith(("Importar", "Import")):
                        # The isolated Recent list contains the profile just exported by this test.
                        subprocess.run(["xdotool", "mousemove", "--window", dialog, "350", "88", "click", "--repeat", "2", "--delay", "100", "1"], check=True)
                    else:
                        key("ctrl+l", "ctrl+a")
                        subprocess.run(["xdotool", "type", "--clearmodifiers", str(path)], check=True)
                        key("Return")
                    time.sleep(.6)
                def wait_calls(command, number):
                    for _ in range(100):
                        if app.poll() is not None:
                            raise AssertionError("GUI exited; inspect gui-smoke.log")
                        path = folder / "calls.jsonl"
                        calls = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
                        if sum(call[0] == command for call in calls) >= number:
                            time.sleep(.2)
                            return calls
                        time.sleep(.05)
                    capture("gui-smoke-failure.png")
                    raise AssertionError(f"No {command} call #{number}; calls: {calls}")
                def settings_check(previous_calls):
                    key("ctrl+5")
                    subprocess.run(["xdotool", "windowsize", window, "1080", "1400"], check=True)
                    time.sleep(.3)
                    click(975, 524)
                    key("Home", "Down", "Down", "Down", "Return")
                    click(975, 576)
                    key("Home", *(["Down"] * 7), "Return")
                    click(845, 620)
                    before = [json.loads(line) for line in (folder / "calls.jsonl").read_text().splitlines()]
                    assert sum(call[0] == "apply-state" for call in before) == previous_calls, "Settings wrote before Apply"
                    subprocess.run(["xdotool", "windowsize", window, "1080", "900"], check=True)
                    time.sleep(.3)
                    click(985, 860)
                    wait_calls("apply-state", previous_calls + 1)
                    changed = json.loads((folder / "state.json").read_text())
                    assert [changed["leds"][i] for i in (3,22,24)] == [3,127,10], changed["leds"]
                    for i in range(128):
                        if i not in (3,22,24): assert changed["leds"][i] == original["leds"][i], i
                    assert changed["layers"] == original["layers"] and changed["macros"] == original["macros"]
                    click(700, 860)
                    wait_calls("apply-state", previous_calls + 2)
                    assert json.loads((folder / "state.json").read_text()) == original
                    capture("gui-settings.png")
                    subprocess.run(["xdotool", "windowsize", window, "1080", "1400"], check=True)
                    time.sleep(.3)
                    click(1000, 945)
                    autostart = folder / "config/autostart/io.github.aula_f87pro.Control.desktop"
                    assert autostart.exists(), "Start at login was not saved"
                    click(1000, 945)
                    assert not autostart.exists(), "Start at login was not disabled"
                    click(970, 996)
                    key("End", "Return")
                    assert json.loads(preference_file.read_text())["minimize_on_close"]
                    click(970, 896)
                    key("Home" if english else "End", "Return")
                    assert json.loads(preference_file.read_text())["english"] != english
                    click(1055, 27)
                    assert app.poll() is None, "Closing in minimize mode quit the application"
                    key("ctrl+q")
                    assert app.wait(timeout=5) == 0, "Explicit Quit did not close the application"

                time.sleep(.6)
                if not (folder / "calls.jsonl").exists():
                    click(960, 95)  # Reload also exercises the mock when no real HID is present.
                wait_calls("snapshot", 1)
                click(250, 645)
                key("Home", "Down", "Down", "Return")
                # Only the board crop should animate; staging must not invoke hardware writes.
                def board_pixels():
                    return subprocess.check_output(["import", "-window", window, "-crop", "980x290+40+180", "rgb:-"])
                first = board_pixels()
                time.sleep(.35)
                if reduced_motion:
                    assert board_pixels() == first, "Lighting preview ignored reduced motion"
                else:
                    assert board_pixels() != first, "Lighting preview did not animate"
                key("alt+a")
                paused = board_pixels()
                time.sleep(.2)
                assert board_pixels() == paused, "Pause control did not stop the preview"
                key("alt+a")
                calls_before = [json.loads(line) for line in (folder / "calls.jsonl").read_text().splitlines()]
                assert all(call[0] == "snapshot" for call in calls_before), "Preview wrote to the backend"
                capture("gui-lighting-preview.png")
                click(985, 860)
                wait_calls("apply-state", 1)
                assert json.loads((folder / "state.json").read_text())["leds"][10] == 2
                click(700, 860)
                wait_calls("apply-state", 2)
                assert json.loads((folder / "state.json").read_text()) == original
                key("ctrl+2")
                capture("gui-keys.png")
                if radio:
                    key("ctrl+4")
                    capture("gui-radio-profiles.png")
                    click(690, 359)  # Load RGB only from the USB profile, preserving radio flags and matrices.
                    click(985, 860)
                    wait_calls("apply-state", 3)
                    state = json.loads((folder / "state.json").read_text())
                    assert state["version"] == 2 and state["leds"][10] == 2 and state["leds"][14] == 1
                    assert state["layers"] == original["layers"] and state["macros"] == original["macros"]
                    capture("gui-radio-preview.png")
                    click(700, 860)
                    wait_calls("apply-state", 4)
                    assert json.loads((folder / "state.json").read_text()) == original
                    key("ctrl+3")
                    capture("gui-radio-macros.png")
                    settings_check(4)
                    (ARTIFACTS / "gui-radio-result.json").write_text(json.dumps({"result": "passed", "hardware": "mock; no HID writes", "checks": ["radio snapshot", "animated RGB preview and pause", "apply and undo LED", "load USB profile RGB while preserving radio data", "undo cross-transport RGB"]}, indent=2))
                    print("Radio GUI smoke passed: lighting, RGB profile conversion and undo; no HID writes.")
                    return
                key("ctrl+3")
                capture("gui-macros-final.png")
                click(700, 280)
                subprocess.run(["xdotool", "type", "--clearmodifiers", example], check=True)
                click(940, 365)
                click(950, 750)
                click(985, 860)
                wait_calls("apply-state", 3)
                macro_state = json.loads((folder / "state.json").read_text())
                assert macro_state["layers"][0][9 * 4:9 * 4 + 4] == [3, 1, 1, 0]
                assert macro_state["macros"][0] == 4
                assert example.encode("utf-16le") in bytes(macro_state["macros"])
                capture("gui-macro-filled.png")
                key("ctrl+4")
                click(250, 414)
                subprocess.run(["xdotool", "type", "--clearmodifiers", example], check=True)
                click(980, 414)
                saved = list((folder / "data/aula-f87pro/profiles").glob("*.json"))
                assert len(saved) == 1
                assert json.loads(saved[0].read_text())["state"] == macro_state
                time.sleep(.4)
                capture("gui-profiles-final.png")
                click(700, 860)
                wait_calls("apply-state", 4)
                assert json.loads((folder / "state.json").read_text()) == original
                click(110, 359)
                click(985, 860)
                wait_calls("apply-state", 5)
                assert json.loads((folder / "state.json").read_text()) == macro_state
                click(700, 860)
                wait_calls("apply-state", 6)
                assert json.loads((folder / "state.json").read_text()) == original
                exported = folder / "exported-profile.json"
                click(405, 359)
                choose_file("Export F87 Pro profile" if english else "Exportar perfil F87 Pro", exported)
                for _ in range(30):
                    if exported.exists():
                        break
                    time.sleep(.1)
                assert exported.exists(), f"The native file picker did not export the profile; files: {list(folder.glob('*'))}"
                assert json.loads(exported.read_text())["state"] == original
                imported = json.loads(exported.read_text())
                imported["name"] = imported_name
                imported["state"] = json.loads(json.dumps(macro_state))
                imported["state"]["layers"][0][9 * 4:9 * 4 + 4] = [3, 2, 1, 0]
                exported.write_text(json.dumps(imported))
                click(245, 359)
                choose_file("Import F87 Pro profile" if english else "Importar perfil F87 Pro", exported)
                click(985, 860)
                wait_calls("apply-state", 7)
                assert json.loads((folder / "state.json").read_text()) == imported["state"]
                key("ctrl+3")
                capture("gui-imported-mode.png")
                key("ctrl+4")
                imported_profiles = [json.loads(path.read_text()) for path in (folder / "data/aula-f87pro/profiles").glob("*.json")]
                assert any(profile["name"] == imported_name and profile["state"] == imported["state"] for profile in imported_profiles)
                click(700, 860)
                wait_calls("apply-state", 8)
                assert json.loads((folder / "state.json").read_text()) == original
                settings_check(8)
                (ARTIFACTS / "gui-smoke-result.json").write_text(json.dumps({"result": "passed", "checks": ["load actual fixture state", "stage and apply selected LED effect", "undo restores complete original snapshot", "create and stage balanced macro", "bind macro and apply complete snapshot", "save named profile", "reload profile restores macro data and bindings", "export through native file picker", "import through native file picker"], "hardware": "mock backend; no HID writes"}, indent=2) + "\n")
                print("GUI smoke passed: state, lighting, undo, macro editor and named profiles; no hardware writes.")
            finally:
                app.terminate()
                app.wait(timeout=5)
                log.flush()
                log_text = (ARTIFACTS / "gui-smoke.log").read_text()
                assert "panicked at" not in log_text and "Gtk-CRITICAL" not in log_text, log_text


if __name__ == "__main__":
    main()
