/* HOI4 Live Tools in-process module: the capability table. See caps.h. */
#include "caps.h"
#include <stdio.h>
#include <string.h>

/* Object field offsets per build. These are the same values the external runtime uses
 * (native/src/game.rs MACOS layout), so stage 0 can show both routes read the same thing. Deriving them by scan
 * (the game's own getter functions and serializer) is stage-1 work; until then an unknown build has reads off. */
static const caps_fields FIELDS[] = {
    {"DFA875A5-25BA-35E0-9736-627F8A5C4396", "1.19.4.1 f584 macOS", 0x428, 0x47c, 0x4d8, 0x679, 0x103717d50, 24},
};

int caps_resolve(const scan_image *im, caps_table *t) {
    memset(t, 0, sizeof *t);
    int ok = 0;
    ok += scan_post_function(im, &t->cap[CAP_POST]);
    ok += scan_queue_root(im, t->cap[CAP_POST].ok ? t->cap[CAP_POST].value : 0, &t->cap[CAP_QUEUE_ROOT]);
    scan_command_factory(im, "CSetResearchCommand", &t->cap[CAP_RESEARCH_FACTORY], &t->cap[CAP_RESEARCH_VTABLE],
                         &t->cap[CAP_RESEARCH_TYPE_ID]);
    ok += t->cap[CAP_RESEARCH_FACTORY].ok + t->cap[CAP_RESEARCH_VTABLE].ok + t->cap[CAP_RESEARCH_TYPE_ID].ok;
    t->cap[CAP_RESEARCH_FACTORY].name = "research_factory";
    t->cap[CAP_RESEARCH_VTABLE].name = "research_vtable";
    t->cap[CAP_RESEARCH_TYPE_ID].name = "research_type_id";
    ok += scan_class_vtable(im, "17CCurrentGameState", &t->cap[CAP_GAMESTATE_VTABLE]);
    t->cap[CAP_GAMESTATE_VTABLE].name = "gamestate_vtable";
    ok += scan_class_vtable(im, "12CInGameIdler", &t->cap[CAP_IDLER_VTABLE]);
    t->cap[CAP_IDLER_VTABLE].name = "idler_vtable";
    scan_cap *f = &t->cap[CAP_FIELDS];
    f->name = "fields";
    snprintf(f->source, sizeof f->source, "table");
    for (size_t i = 0; i < sizeof FIELDS / sizeof FIELDS[0]; i++)
        if (!strcmp(FIELDS[i].uuid, im->uuid)) t->fields = &FIELDS[i];
    f->ok = t->fields != NULL;
    snprintf(f->evidence, sizeof f->evidence, "%s", f->ok ? t->fields->version : "no field table for this build: reads off");
    ok += f->ok;
    return ok;
}

const char *caps_json(const scan_image *im, const caps_table *t, char *buf, size_t n) {
    size_t at = (size_t)snprintf(buf, n, "{\"uuid\":\"%s\",\"capabilities\":[", im->uuid);
    for (int i = 0; i < CAP_COUNT && at < n; i++) {
        char one[512];
        at += (size_t)snprintf(buf + at, n - at, "%s%s", i ? "," : "", scan_cap_json(&t->cap[i], one, sizeof one));
    }
    if (at < n) snprintf(buf + at, n - at, "]}");
    return buf;
}
