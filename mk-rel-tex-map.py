from collections import defaultdict
import shutil
import os
from pathlib import Path

tex_path = Path("assets/textures/")
tex_files = tex_path.glob("*.webp")

main_tex_files = defaultdict(dict)

for tex_file in tex_files:
    stem = tex_file.stem
    if stem.endswith("UV"):
        main_tex_files[stem[:-2]]["UV"] = tex_file
    elif stem.endswith("IR"):
        main_tex_files[stem[:-2]]["IR"] = tex_file

missing_one = set()

for stem, files in main_tex_files.items():
    missing_uv = "UV" not in files
    missing_ir = "IR" not in files
    if missing_uv or missing_ir:
        missing_one.add(stem)
        # print(f"Missing textures for {stem}: {'UV' if missing_uv else ''}{'IR' if missing_ir else ''}")
        # files["UV"] = files.get("UV", files.get("IR", None))
        # files["IR"] = files.get("IR", files.get("UV", None))
        # if not (files["UV"] and files["IR"]):
        #     print(f"Something went wrong {stem} => {files}")
        #     continue

match_lines = []

for stem in missing_one:
    files = main_tex_files[stem]
    missing_uv = "UV" not in files
    missing_ir = "IR" not in files
    if missing_uv and missing_ir:
        print(f"Both UV and IR textures are missing for {stem}")
        continue
    elif missing_uv:
        files["UV"] = files["IR"]
        match_lines.append(f"(\"{stem}\", RelativisticTextureType::UV) => \"{files['UV']}\",")
    elif missing_ir:
        files["IR"] = files["UV"]
        match_lines.append(f"(\"{stem}\", RelativisticTextureType::IR) => \"{files['IR']}\",")

if match_lines:
    print("Add the following lines to the lookup_rel_texture function:")
    for line in match_lines:
        print(line)
