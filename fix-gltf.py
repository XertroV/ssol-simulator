import json
import os
from pathlib import Path


def fix_materials(data, gltf_file_path, textures_dir="assets/textures", texture_extension=".png", new_extension=".webp"):
    textures_path = Path(textures_dir)

    # --- Build the new image and texture lists ---
    new_images = []
    new_textures = []
    material_name_to_texture_index = {}

    for i, material in enumerate(data.get("materials", [])):
        material_name = material.get("name")
        if not material_name:
            print(f"  Warning: Material at index {i} has no name. Skipping.")
            continue

        texture_name = material_name.replace(" (Instance)", "")

        # Construct the expected texture filename from the material name
        texture_filename = f"{texture_name}{texture_extension}"
        texture_file_path = textures_path / texture_filename
        new_texture_filename = f"{texture_name}{new_extension}"

        # The URI path should be relative to the assets folder for Bevy
        texture_uri = f"../textures/{new_texture_filename}"

        if texture_file_path.is_file():
            print(f"  Found matching texture for material '{material_name}': {texture_uri}")

            # Add the image and texture info if we haven't already
            if material_name not in material_name_to_texture_index:
                image_index = len(new_images)
                new_images.append({"uri": texture_uri})
                new_textures.append({"source": image_index})
                material_name_to_texture_index[material_name] = image_index
        else:
            print(f"  Warning: No matching texture found for material '{material_name}' at '{texture_file_path}'")

    # --- Update the materials to point to the new textures ---
    for material in data.get("materials", []):
        material_name = material.get("name")
        if material_name in material_name_to_texture_index:
            texture_index = material_name_to_texture_index[material_name]

            # Add the PBR info. This is the crucial link.
            material["pbrMetallicRoughness"] = {
                "baseColorTexture": {
                    "index": texture_index
                }
            }
            # We can also remove the old alphaCutoff if it's not needed
            # material.pop("alphaCutoff", None)


    # --- Add the new lists to the main glTF data ---
    if new_images:
        data["images"] = new_images
        data["textures"] = new_textures
        return True
    return False


def fix_nodes(data):
    updated = False
    if "nodes" not in data:
        return False

    known_children = set()

    for i, node in enumerate(data["nodes"]):
        if "children" in node:
            known_children.update(node["children"])
        if i in known_children:
            continue
        name = node.get("name", "Unnamed")
        if "translation" in node and "translation_orig" not in node:
            if "translation_orig" in node:
                continue
            # print(f"  Warning: Node '{name}' has translation {node['translation']}. This may cause issues.")
            node["translation_orig"] = node["translation"]
            node["translation"] = [0.0, 0.0, 0.0]
            updated = True
        if "scale" in node and "scale_orig" not in node:
            if "scale_orig" in node:
                continue
            # print(f"  Warning: Node '{name}' has scale {node['scale']}. This may cause issues.")
            node["scale_orig"] = node["scale"]
            node["scale"] = [1.0, 1.0, 1.0]
            updated = True
        if "rotation" in node and "rotation_orig" not in node:
            if "rotation_orig" in node:
                continue
            # print(f"  Warning: Node '{name}' has rotation {node['rotation']}. This may cause issues.")
            node["rotation_orig"] = node["rotation"]
            node["rotation"] = [0.0, 0.0, 0.0, 1.0]
            updated = True

    return updated


def patch_gltf_files(models_dir="assets/models", textures_dir="assets/textures", texture_extension=".png"):
    """
    Scans for .gltf files in a directory and automatically links materials
    to textures based on matching names.

    Args:
        models_dir (str): The relative path to the directory containing .gltf files.
        textures_dir (str): The relative path to the directory containing texture files.
        texture_extension (str): The file extension of the textures (e.g., ".png").
    """
    models_path = Path(models_dir)
    textures_path = Path(textures_dir)

    if not models_path.is_dir():
        print(f"Error: Models directory not found at '{models_path}'")
        return

    if not textures_path.is_dir():
        print(f"Error: Textures directory not found at '{textures_path}'")
        return

    print(f"Scanning for .gltf files in '{models_path}'...")

    for gltf_file_path in models_path.glob("*.gltf"):
        print(f"\n--- Processing: {gltf_file_path.name} ---")

        try:
            with open(gltf_file_path, 'r') as f:
                data = json.load(f)
        except (json.JSONDecodeError, IOError) as e:
            print(f"  Error reading or parsing file: {e}")
            continue

        updated = False

        if "materials" in data or data["materials"]:
            updated = fix_materials(data, gltf_file_path, textures_dir, texture_extension) or updated

        if "nodes" in data:
            updated = fix_nodes(data) or updated

        if updated:
            # --- Write the changes back to the file ---
            try:
                with open(gltf_file_path, 'w') as f:
                    json.dump(data, f, indent=2)
                print(f"  Successfully patched and saved {gltf_file_path.name}")
            except IOError as e:
                print(f"  Error writing to file: {e}")
        else:
            print("  No valid textures found to apply. No changes made.")

if __name__ == "__main__":
    patch_gltf_files()
    print("\nScript finished.")
