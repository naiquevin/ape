import os
import shutil
from datetime import datetime

BASE_PATH = "/data/project"


def backup_user_config():
    src = os.path.join(BASE_PATH, "config", "user_config.json")
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = os.path.join(BASE_PATH, "backup", f"user_config_{timestamp}.json")
    if not os.path.exists(src):
        print(f"[ERROR] Source file not found: {src}")
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    print(f"[INFO] Backed up {src} → {dst}")
    return True

def backup_app_config():
    src = os.path.join(BASE_PATH, "config", "app_config.json")
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = os.path.join(BASE_PATH, "backup", f"app_config_{timestamp}.json")
    if not os.path.exists(src):
        print(f"[ERROR] Source file not found: {src}")
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    print(f"[INFO] Backed up {src} → {dst}")
    return True

def backup_network_config():
    src = os.path.join(BASE_PATH, "config", "network_config.json")
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = os.path.join(BASE_PATH, "backup", f"network_config_{timestamp}.json")
    if not os.path.exists(src):
        print(f"[ERROR] Source file not found: {src}")
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    print(f"[INFO] Backed up {src} → {dst}")
    return True


if __name__ == "__main__":
    backup_user_config()
    backup_app_config()
    backup_network_config()
