# Object slots: RSLT (0xD3044521)

Referenced from an object's VPXY (its TGI list; not the MODL's instance). An RCOL with one
`RSLT` chunk. Patched copies in DeltaBuild packages override FullBuild ones (and may add slots).

```
char[4] "RSLT"
u32 version (4)
u32 nRouting, nContainer, nEffect, nIKTarget, nCone
for each kind in that order, when its count > 0:
    u32 name[n]            // FNV-32 hashes (not of "_FX_0" etc.; names unknown)
    u32 bone[n]            // the object rig's bone the slot hangs off
    u32 flags[n]           // containers only
    f32 matrix[n][12]      // 3x4 row-major: rotation rows, translation in the last column
    u32 nOffsets
    nOffsets x { u32 slot; f32 position[3]; f32 rotation[3] }   // 28 bytes
```

Verified on ShowerBasic (shower head at (0, 1.86, -0.28)), the Plaza Gusher (nozzle 2.19 m up),
sinks (tap ~1 m up at the back), stoves (burner, oven), fireplaces (the hearth), a dining table
(9 container slots). Objects face +Z in model space. The bake keeps each object's effect slot
positions (`GameDataBaked::fx_slots`).
