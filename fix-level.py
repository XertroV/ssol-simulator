import json
import os
from pathlib import Path

NOT_NEEDED = set(["pCube", "group", "Long_Pole", "polySurface", "leftTop", "leftB", "rightTop", "rightB", "transform"])
def remove_unnecessary_objects(data: list[dict]):
    for i, obj in list(enumerate(data))[::-1]:
        if obj.get("sceneName") == "LevelZero":
            del obj["sceneName"]
        if obj.get("tag") == "Untagged":
            del obj["tag"]
        if any(obj["name"].startswith(prefix) for prefix in NOT_NEEDED):
            print(f"Removing unnecessary object: {obj['name']}")
            o = data.pop(i)
            print(f"Removed object: {o['name']}")
            continue
            # o = data.pop(i)
            # print(f"Removed object: {o['name']}")
            # continue

def main():
    scene_file = Path("assets/scenes/level-zero.json")
    if not scene_file.is_file():
        print(f"Scene file '{scene_file}' does not exist.")
        return
    with open(scene_file, 'r', encoding='utf-8') as f:
        data = json.load(f)
    remove_unnecessary_objects(data)
    with open(scene_file, 'w', encoding='utf-8') as f:
        json.dump(data, f, ensure_ascii=False, indent=None)
    print(f"Updated scene file '{scene_file}' successfully. Objects: {len(data)}")

if __name__ == "__main__":
    main()
