"""Launch using ignored local configuration, without copying keys or printing values."""
import json
import os
from pathlib import Path
import sys
from urllib.request import urlopen

HERE = Path(__file__).resolve().parent
CONFIG = HERE.parents[3] / "secrets/voice-recorder/connection.json"
if not CONFIG.is_file():
    sys.exit("Создайте secrets/voice-recorder/connection.json по README инструмента.")
config = json.loads(CONFIG.read_text())
if not any(a in sys.argv for a in ["--recover-device", "--remove-device"]):
    url = "http://127.0.0.1:" + str(config.get("port", 8765))
    try:
        with urlopen(url, timeout=1) as response:
            existing = response.headers.get("X-Ricardo-Recorder") == "local-v1"
    except OSError:
        existing = False
    if existing:
        import webbrowser
        webbrowser.open(url)
        print("Локальный интерфейс уже запущен; открыта его страница.")
        sys.exit(0)
auth = ["--ssh-key", str(Path(config["ssh_key"]).expanduser())] if "ssh_key" in config else ["--credentials", str(Path(config["credentials"]).expanduser())]
args = [sys.executable, str(HERE / "server.py"), "--station", config["station"],
        "--ssh-target", config["ssh_target"], *auth,
        "--ssh-port", str(config.get("ssh_port", 22)), "--port", str(config.get("port", 8765)), *sys.argv[1:]]
if not any(a in sys.argv for a in ["--recover-device", "--remove-device"]):
    args.append("--open-browser")
os.execv(sys.executable, args)
