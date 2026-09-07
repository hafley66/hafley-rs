# Wireframe fighter lab

Run `bash 5_run.sh` here to rebuild the Blender model, export glTF, import into
Godot, verify the skeleton and pose restoration, and record H.264 MP4.

- Editable model: `1_wire_fighter.blend`.
- Engine asset: `2_wire_fighter.glb`.
- Recorded output: `7_wire_fighter.mp4` (6 seconds, 30 FPS, 800x600).
- Frame inspection: `8_frames.png`.
- Rig: 16 bones, one weighted wire mesh, `PunchKnee` clip.

Geometry and keyframes were authored by this lab's Blender script. This is a
Falcon-style wireframe humanoid prototype, not an extracted game model or a
reproduction of Melee frame data. Godot currently evaluates its animation.
Rust-driven bone poses, bone-attached collision volumes, and gameplay/rollback
integration for this fighter are not implemented by this asset proof.

Godot direct .blend import is disabled because the script explicitly exports
.glb. Rendering uses a Node's process callback so scene transforms update before
capture. Tested on Blender 5.2.1, Godot 4.7 and Apple M2 Pro.
