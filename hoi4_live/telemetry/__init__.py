"""The telemetry companion mod (hoi4live_telemetry): generation, playset management and game.log parsing.

    modgen.py    writes the mod (descriptor, on_actions, scripted effects, localisation) into the user mod folder
    playset.py   adds it to / removes it from dlc_load.json, with a backup of the original file
    parser.py    turns `HOI4LIVE {json}` lines of game.log into records and a queryable store
"""
