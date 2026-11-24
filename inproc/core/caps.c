/* HOI4 Live Tools in-process module: the capability table. See caps.h. */
#include "caps.h"
#include <stdio.h>
#include <string.h>

/* Object field offsets per build. These are the same values the external runtime uses
 * (native/src/game.rs MACOS layout), so stage 0 can show both routes read the same thing. Deriving them by scan
 * (the game's own getter functions and serializer) is stage-1 work; until then an unknown build has reads off. */
static const caps_fields FIELDS[] = {
    {"DFA875A5-25BA-35E0-9736-627F8A5C4396", "1.19.4.1 f584 macOS", 0x428, 0x47c, 0x4d8, 0x679, 0x103717d50, 24},
    /* 1.20.0.0 0c4f: every field unchanged, the tag array root moved (build migration map, matcher:global, high) */
    {"ED135FE5-CDA8-38E8-8835-3F7381B79974", "1.20.0.0 0c4f macOS", 0x428, 0x47c, 0x4d8, 0x679, 0x10371ced0, 24},
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
    scan_cap *c = &t->cap[CAP_POST_LAYOUT];
    c->name = "post_layout"; snprintf(c->source, sizeof c->source, "scan");
    c->ok = scan_post_layout(im, t->cap[CAP_POST].ok ? t->cap[CAP_POST].value : 0, &t->post);
    c->value = t->post.auth_slot; c->extra = t->post.sender_src;
    snprintf(c->evidence, sizeof c->evidence, "%.159s", t->post.evidence);
    ok += c->ok;
    c = &t->cap[CAP_SPEED_CMD];
    c->name = "speed_command"; snprintf(c->source, sizeof c->source, "export+rtti+scan");
    c->ok = scan_speed_command(im, &t->post, &t->speed);
    c->value = t->speed.type_id; c->extra = t->speed.payload_off;
    snprintf(c->evidence, sizeof c->evidence, "%.159s", t->speed.evidence);
    ok += c->ok;
    c = &t->cap[CAP_FORBID_TLV];
    c->name = "forbid_tlv"; snprintf(c->source, sizeof c->source, "scan");
    c->ok = scan_forbid_tlv(im, &t->forbid);
    c->value = t->forbid.accessor; c->extra = t->forbid.desc;
    snprintf(c->evidence, sizeof c->evidence, "%.159s", t->forbid.evidence);
    ok += c->ok;
    c = &t->cap[CAP_STAGE2_COMMON];
    c->name = "stage2_common"; snprintf(c->source, sizeof c->source, "scan");
    int on = scan_stage2(im, &t->post, t->speed.ok ? t->speed.gs_global : 0, &t->st2);
    c->ok = t->st2.common.ok; c->value = (uint64_t)on; c->extra = (uint64_t)t->st2.n;
    snprintf(c->evidence, sizeof c->evidence, "%.159s", t->st2.common.evidence);
    ok += c->ok;
    return ok;
}

const char *caps_json(const scan_image *im, const caps_table *t, char *buf, size_t n) {
    size_t at = (size_t)snprintf(buf, n, "{\"uuid\":\"%s\",\"capabilities\":[", im->uuid);
    for (int i = 0; i < CAP_COUNT && at < n; i++) {
        char one[512];
        at += (size_t)snprintf(buf + at, n - at, "%s%s", i ? "," : "", scan_cap_json(&t->cap[i], one, sizeof one));
    }
    if (at < n)
        at += (size_t)snprintf(buf + at, n - at,
                 "],\"speed_command\":{\"ok\":%s,\"factory\":\"0x%llx\",\"vtable\":\"0x%llx\",\"type_id\":%u,"
                 "\"size\":%u,\"slots\":%u,\"payload_off\":%u,\"range_max\":%u,\"field_id\":%u,\"required_sender\":%u,"
                 "\"speed_field\":%u,\"gs_global\":\"0x%llx\"},\"post_layout\":{\"ok\":%s,\"gate_flag_off\":%u,"
                 "\"gate_slot\":%u,\"sender_src\":%u,\"server_off\":%u,\"send_slot\":%u,\"auth_slot\":%u}}",
                 t->speed.ok ? "true" : "false", (unsigned long long)t->speed.factory,
                 (unsigned long long)t->speed.vtable, t->speed.type_id, t->speed.size, t->speed.slots,
                 t->speed.payload_off, t->speed.range_max, t->speed.field_id, t->speed.required_sender,
                 t->speed.speed_field, (unsigned long long)t->speed.gs_global, t->post.ok ? "true" : "false",
                 t->post.gate_flag_off, t->post.gate_slot, t->post.sender_src, t->post.server_off, t->post.send_slot,
                 t->post.auth_slot);
    if (at < n - 1 && buf[at - 1] == '}') { /* append the stage-2 table inside the object */
        at--;
        at += (size_t)snprintf(buf + at, n - at, ",\"stage2\":");
        if (at < n) { stage2_json(&t->st2, buf + at, n - at); at += strlen(buf + at); }
        if (at < n) snprintf(buf + at, n - at, "}");
    }
    return buf;
}
