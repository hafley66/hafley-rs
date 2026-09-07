import bpy
import math
from pathlib import Path

root = Path(__file__).resolve().parent
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
bpy.ops.object.armature_add()
rig = bpy.context.object
rig.name = 'ProofRig'
rig.data.bones[0].name = 'Arm'
bpy.ops.mesh.primitive_cube_add(location=(0, 0, 1))
mesh = bpy.context.object
mesh.name = 'AnimatedArm'
mesh.scale = (0.2, 0.2, 1)
bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
material = bpy.data.materials.new('Orange')
material.diffuse_color = (1, 0.25, 0.03, 1)
mesh.data.materials.append(material)
group = mesh.vertex_groups.new(name='Arm')
group.add(list(range(len(mesh.data.vertices))), 1.0, 'REPLACE')
modifier = mesh.modifiers.new('Skin', 'ARMATURE')
modifier.object = rig
mesh.parent = rig
bone = rig.pose.bones['Arm']
bone.rotation_mode = 'XYZ'
for frame, angle in [(1, 0), (16, math.pi / 2), (31, 0)]:
    bone.rotation_euler[1] = angle
    bone.keyframe_insert(data_path='rotation_euler', frame=frame)
rig.animation_data.action.name = 'Swing'
bpy.context.scene.frame_start = 1
bpy.context.scene.frame_end = 31
bpy.context.scene.render.fps = 30
bpy.context.scene.frame_set(1)
bpy.ops.wm.save_as_mainfile(filepath=str(root / '1_model.blend'))
bpy.ops.export_scene.gltf(filepath=str(root / 'godot' / '1_model.glb'), export_format='GLB')
print('BLENDER_PROOF_EXPORTED')
