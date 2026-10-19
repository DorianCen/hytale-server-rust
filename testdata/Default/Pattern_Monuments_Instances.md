# Pattern_Monuments in Grid Generators & Entry Points

## Grid Generator Files:
1. [Zones/Noise/Shared/Grids/Monuments_Encounter.json](Zones/Noise/Shared/Grids/Monuments_Encounter.json#L3) - Main grid definition with warning comment

## Shared PrefabPattern Files (Grid-based):
2. [Zones/PrefabPatterns_Shared/Monuments_Encounter.json](Zones/PrefabPatterns_Shared/Monuments_Encounter.json#L3)
3. [Zones/PrefabPatterns_Shared/Monuments_Factions.json](Zones/PrefabPatterns_Shared/Monuments_Factions.json#L3)
4. [Zones/PrefabPatterns_Shared/Monuments_Factions_Ferans.json](Zones/PrefabPatterns_Shared/Monuments_Factions_Ferans.json#L3)
5. [Zones/PrefabPatterns_Shared/Monuments_Factions_Outlanders.json](Zones/PrefabPatterns_Shared/Monuments_Factions_Outlanders.json#L3)
6. [Zones/PrefabPatterns_Shared/Monuments_Factions_Outlanders_Tier3.json](Zones/PrefabPatterns_Shared/Monuments_Factions_Outlanders_Tier3.json#L3)

## Faction NoiseMask Files (Grid-based):
9. [Zones/Noise/Zone1/NoiseMasks/Faction_Trorks.json](Zones/Noise/Zone1/NoiseMasks/Faction_Trorks.json#L4)
10. [Zones/Noise/Zone1/NoiseMasks/Faction_Kweebecs.json](Zones/Noise/Zone1/NoiseMasks/Faction_Kweebecs.json#L4)
11. [Zones/Noise/Zone1/NoiseMasks/Dungeons.json](Zones/Noise/Zone1/NoiseMasks/Dungeons.json#L2)
12. [Zones/Noise/Zone1/NoiseMasks/Faction_Undead.json](Zones/Noise/Zone1/NoiseMasks/Faction_Undead.json#L2)
13. [Zones/Noise/Zone3/NoiseMasks/Faction_Outlanders.json](Zones/Noise/Zone3/NoiseMasks/Faction_Outlanders.json#L2)
14. [Zones/Noise/Zone3/NoiseMasks/Faction_Kweebecs.json](Zones/Noise/Zone3/NoiseMasks/Faction_Kweebecs.json#L2)

## Cave Entry Points - Zone 1 Tier 1:
13. [Zone1_Tier1/Cave/Caves.json](Zones/Zone1_Tier1/Cave/Caves.json#L271) - Dungeons_Cave
14. [Zone1_Tier1/Cave/Caves.json](Zones/Zone1_Tier1/Cave/Caves.json#L317) - Dungeons
15. [Zone1_Tier1/Cave/Caves.json](Zones/Zone1_Tier1/Cave/Caves.json#L361) - Encounter

## Cave Entry Points - Zone 1 Tier 2:
16. [Zone1_Tier2/Cave/Caves.json](Zones/Zone1_Tier2/Cave/Caves.json#L273) - Dungeons_Cave
17. [Zone1_Tier2/Cave/Caves.json](Zones/Zone1_Tier2/Cave/Caves.json#L319) - Dungeons
18. [Zone1_Tier2/Cave/Caves.json](Zones/Zone1_Tier2/Cave/Caves.json#L363) - Encounter

## Cave Entry Points - Zone 1 Tier 3:
19. [Zone1_Tier3/Cave/Caves.json](Zones/Zone1_Tier3/Cave/Caves.json#L109) - Dungeons_Swamp
20. [Zone1_Tier3/Cave/Caves.json](Zones/Zone1_Tier3/Cave/Caves.json#L156) - Dungeons_Swamp_Cave
21. [Zone1_Tier3/Cave/Caves.json](Zones/Zone1_Tier3/Cave/Caves.json#L273) - Dungeons_Cave
22. [Zone1_Tier3/Cave/Caves.json](Zones/Zone1_Tier3/Cave/Caves.json#L319) - Dungeons
23. [Zone1_Tier3/Cave/Caves.json](Zones/Zone1_Tier3/Cave/Caves.json#L363) - Encounter

## Cave Entry Points - Zone 3 Tier 1:
24. [Zone3_Tier1/Cave/Caves.json](Zones/Zone3_Tier1/Cave/Caves.json#L179) - Dungeons_Cave
25. [Zone3_Tier1/Cave/Caves.json](Zones/Zone3_Tier1/Cave/Caves.json#L226) - Dungeons

## Cave Entry Points - Zone 3 Tier 2:
26. [Zone3_Tier2/Cave/Caves.json](Zones/Zone3_Tier2/Cave/Caves.json#L179) - Dungeons_Cave
27. [Zone3_Tier2/Cave/Caves.json](Zones/Zone3_Tier2/Cave/Caves.json#L226) - Dungeons

## Cave Entry Points - Zone 3 Tier 3:
28. [Zone3_Tier3/Cave/Caves.json](Zones/Zone3_Tier3/Cave/Caves.json#L19) - Dungeons_Outlander_Mountain
29. [Zone3_Tier3/Cave/Caves.json](Zones/Zone3_Tier3/Cave/Caves.json#L65) - Dungeons_Outlander_Temple
30. [Zone3_Tier3/Cave/Caves.json](Zones/Zone3_Tier3/Cave/Caves.json#L152) - Dungeons_Challenge
31. [Zone3_Tier3/Cave/Caves.json](Zones/Zone3_Tier3/Cave/Caves.json#L202) - Dungeons_Cave
32. [Zone3_Tier3/Cave/Caves.json](Zones/Zone3_Tier3/Cave/Caves.json#L278) - Dungeons

---

**Total: 34 instances** requiring manual coordination when updating Pattern_Monuments parameters.

**Note:** The file [Zones/Noise/Shared/Grids/Monuments_Encounter.json](Zones/Noise/Shared/Grids/Monuments_Encounter.json#L2) contains an explicit comment at line 2:
```
"|||| GRID GENERATORS & ENTRY POINTS & FACTION NOISEMAPS NEEDS TO BE UPDATED MANUALLY ||||"
```

This indicates these configurations are intentionally using a shared seed and should be updated together manually when changes are made.

