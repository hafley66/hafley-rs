import bpy
import math
from mathutils import Vector
from pathlib import Path

ROOT = Path(__file__).resolve().parent
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
bpy.ops.object.armature_add()
rig = bpy.context.object
rig.name = 'WireFighterRig'
bpy.ops.object.mode_set(mode='EDIT')
rig.data.edit_bones.remove(rig.data.edit_bones[0])
bone_specs = [
    ('root', (0, 0, 0), (0, 0, 1.1), None),
    ('pelvis', (0, 0, 1.1), (0, 0, 1.3), 'root'),
    ('spine', (0, 0, 1.3), (0, 0, 1.78), 'pelvis'),
    ('head', (0, 0, 1.78), (0, 0, 2.12), 'spine'),
]
for side, sign in [('L', -1), ('R', 1)]:
    bone_specs.extend([
        (f'upper_arm_{side}', (.24 * sign, 0, 1.72), (.52 * sign, 0, 1.43), 'spine'),
        (f'forearm_{side}', (.52 * sign, 0, 1.43), (.65 * sign, -.03, 1.16), f'upper_arm_{side}'),
        (f'hand_{side}', (.65 * sign, -.03, 1.16), (.68 * sign, -.06, 1.02), f'forearm_{side}'),
        (f'thigh_{side}', (.15 * sign, 0, 1.12), (.19 * sign, 0, .62), 'pelvis'),
        (f'shin_{side}', (.19 * sign, 0, .62), (.2 * sign, 0, .16), f'thigh_{side}'),
        (f'foot_{side}', (.2 * sign, 0, .16), (.2 * sign, -.24, .08), f'shin_{side}'),
    ])
for name, head, tail, parent in bone_specs:
    bone = rig.data.edit_bones.new(name)
    bone.head, bone.tail = head, tail
    if parent:
        bone.parent = rig.data.edit_bones[parent]
bpy.ops.object.mode_set(mode='OBJECT')
parts = []

def weighted_part(obj, bone_name):
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    group = obj.vertex_groups.new(name=bone_name)
    group.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
    parts.append(obj)

def ellipsoid(name, location, scale, bone_name):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=8, ring_count=5, location=location)
    obj = bpy.context.object
    obj.name, obj.scale = name, scale
    weighted_part(obj, bone_name)

ellipsoid('Chest', (0, 0, 1.56), (.3, .17, .3), 'spine')
ellipsoid('Pelvis', (0, 0, 1.18), (.23, .15, .18), 'pelvis')
ellipsoid('Helmet', (0, -.015, 1.99), (.17, .155, .22), 'head')
ellipsoid('Visor', (0, -.15, 2.02), (.16, .045, .065), 'head')
for name, head, tail, parent in bone_specs[4:]:
    a, b = Vector(head), Vector(tail)
    radii = {'upper': .095, 'forearm': .085, 'hand': .085, 'thigh': .115, 'shin': .085, 'foot': .1}
    radius = radii[name.split('_')[0]]
    bpy.ops.mesh.primitive_cone_add(vertices=8, radius1=radius * .75, radius2=radius, depth=(b-a).length, location=(a+b)/2)
    obj = bpy.context.object
    obj.name = name + '_mesh'
    obj.rotation_mode = 'QUATERNION'
    obj.rotation_quaternion = Vector((0, 0, 1)).rotation_difference((b-a).normalized())
    weighted_part(obj, name)
    ellipsoid(name + '_joint', a, (radius, radius, radius), name)
bpy.ops.object.select_all(action='DESELECT')
for obj in parts:
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
bpy.ops.object.join()
mesh = bpy.context.object
mesh.name = 'WireFighterSkin'
# Bake only the visible edge geometry. Bone weights remain on the exported mesh.
wire = mesh.modifiers.new('VisibleWireEdges', 'WIREFRAME')
wire.thickness = .012
wire.use_replace = True
bpy.ops.object.modifier_apply(modifier=wire.name)
material = bpy.data.materials.new('VioletWire')
material.use_nodes = True
shader = material.node_tree.nodes.get('Principled BSDF')
shader.inputs['Base Color'].default_value = (.55, .04, 1, 1)
shader.inputs['Emission Color'].default_value = (.42, .025, .9, 1)
shader.inputs['Emission Strength'].default_value = 1.6
shader.inputs['Roughness'].default_value = .5
mesh.data.materials.append(material)
armature = mesh.modifiers.new('SkeletalDeformation', 'ARMATURE')
armature.object = rig
mesh.parent = rig
poses = {
    1: {},
    16: {'spine': (.05, -.25, 0), 'forearm_L': (-1.1, 0, 0), 'forearm_R': (-1.2, 0, 0)},
    31: {'spine': (.05, .35, 0), 'upper_arm_R': (-1.4, 0, -.35), 'forearm_R': (-.1, 0, 0), 'forearm_L': (-1.2, 0, 0)},
    46: {'forearm_L': (-1.0, 0, 0), 'forearm_R': (-1.0, 0, 0)},
    61: {'spine': (-.15, .1, 0), 'thigh_R': (-1.5, 0, 0), 'shin_R': (1.8, 0, 0), 'forearm_L': (-1.2, 0, 0), 'forearm_R': (-1.2, 0, 0)},
    76: {'forearm_L': (-.7, 0, 0), 'forearm_R': (-.7, 0, 0)},
    91: {},
}
for frame, rotations in poses.items():
    for bone in rig.pose.bones:
        bone.rotation_mode = 'XYZ'
        bone.rotation_euler = rotations.get(bone.name, (0, 0, 0))
        bone.keyframe_insert('rotation_euler', frame=frame)
rig.animation_data.action.name = 'PunchKnee'
scene = bpy.context.scene
scene.render.fps = 30
scene.frame_start, scene.frame_end = 1, 91
scene.frame_set(1)
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / '1_wire_fighter.blend'))
bpy.ops.export_scene.gltf(filepath=str(ROOT / '2_wire_fighter.glb'), export_format='GLB', export_animations=True)
assert len(rig.data.bones) == 16
print(f'WIRE_FIGHTER_EXPORTED bones={len(rig.data.bones)} vertices={len(mesh.data.vertices)} clip=PunchKnee')
