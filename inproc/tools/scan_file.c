/* scan_file: run the module's scanner over an executable FILE (offline; never touches a running game).
 * usage: scan_file EXE [name=0xVALUE ...]   prints the capability table as JSON; with expectations, exits 1 on any
 * mismatch. Names: command_post, queue_root, queue_slot, research_factory, research_vtable, research_type_id,
 * gamestate_vtable, idler_vtable, fields (1 = a field table exists for this build). */
#include "../core/caps.h"
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec / 1e9; }

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: scan_file EXE [name=0xVALUE ...]\n"); return 2; }
    int fd = open(argv[1], O_RDONLY);
    struct stat st;
    if (fd < 0 || fstat(fd, &st)) { perror(argv[1]); return 2; }
    const uint8_t *map = mmap(NULL, (size_t)st.st_size, PROT_READ, MAP_PRIVATE, fd, 0);
    if (map == MAP_FAILED) { perror("mmap"); return 2; }
    scan_image im; caps_table t;
    double t0 = now();
    if (!scan_image_open_file(&im, map, (size_t)st.st_size)) { fprintf(stderr, "not an x86_64 Mach-O image\n"); return 2; }
    if (getenv("SCAN_AS_PROCESS")) {
        /* Lay the segments out as dyld would, at a nonzero slide, and rescan through the process view: the exact
         * code path the module uses on its own memory (slide translation, __LINKEDIT through its segment). */
        uint64_t lo = UINT64_MAX, hi = 0;
        for (int i = 0; i < im.nsegs; i++)
            if (im.segs[i].vmsize && strcmp(im.segs[i].name, "__PAGEZERO")) {
                if (im.segs[i].vmaddr < lo) lo = im.segs[i].vmaddr;
                if (im.segs[i].vmaddr + im.segs[i].vmsize > hi) hi = im.segs[i].vmaddr + im.segs[i].vmsize;
            }
        uint8_t *mem = mmap(NULL, (size_t)(hi - lo), PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
        if (mem == MAP_FAILED) { perror("mmap anon"); return 2; }
        for (int i = 0; i < im.nsegs; i++)
            if (im.segs[i].filesize && strcmp(im.segs[i].name, "__PAGEZERO"))
                memcpy(mem + (im.segs[i].vmaddr - lo), map + im.segs[i].fileoff, (size_t)im.segs[i].filesize);
        int64_t slide = (int64_t)((uint64_t)(uintptr_t)mem - lo);
        scan_image_free(&im);
        if (!scan_image_open_process(&im, mem, slide)) { fprintf(stderr, "process view failed\n"); return 2; }
        fprintf(stderr, "process view: segments laid out at %p, slide 0x%llx\n", (void *)mem, (unsigned long long)slide);
    }
    int ok = caps_resolve(&im, &t);
    double t1 = now();
    uint64_t *g; unsigned *r;
    size_t ng = scan_global_candidates(&im, 200, &g, &r);
    double t2 = now();
    static char buf[65536];
    printf("%s\n", caps_json(&im, &t, buf, sizeof buf));
    printf("{\"resolved\":%d,\"of\":%d,\"scan_ms\":%.1f,\"globals_ms\":%.1f,\"globals_200_refs\":%zu,\"top\":[",
           ok, CAP_COUNT, (t1 - t0) * 1e3, (t2 - t1) * 1e3, ng);
    for (size_t i = 0; i < ng && i < 6; i++) printf("%s[\"0x%llx\",%u]", i ? "," : "", (unsigned long long)g[i], r[i]);
    long qrank = -1;
    for (size_t i = 0; i < ng; i++) if (g[i] == t.cap[CAP_QUEUE_ROOT].value) qrank = (long)i;
    printf("],\"queue_root_rank\":%ld}\n", qrank);
    { static char db[16384]; printf("{%s}\n", stage2_diplo_json(&t.st2, db, sizeof db)); } /* engineer 122: the diplomacy catalog */
    int bad = 0;
    for (int a = 2; a < argc; a++) {
        char *eq = strchr(argv[a], '=');
        if (!eq) continue;
        *eq = 0;
        uint64_t want = strtoull(eq + 1, NULL, 0), got = 0; int found = 1;
        if (!strcmp(argv[a], "queue_slot")) got = t.cap[CAP_QUEUE_ROOT].extra;
        else if (!strcmp(argv[a], "auth_slot")) got = t.post.ok ? t.post.auth_slot : 0;
        else if (!strcmp(argv[a], "speed_type_id")) got = t.speed.ok ? t.speed.type_id : 0;
        else if (!strcmp(argv[a], "speed_payload")) got = t.speed.ok ? t.speed.payload_off : 0;
        else if (!strcmp(argv[a], "speed_field")) got = t.speed.ok ? t.speed.speed_field : 0;
        else if (!strcmp(argv[a], "speed_range")) got = t.speed.ok ? t.speed.range_max : 0;
        else if (!strcmp(argv[a], "speed_gs_global")) got = t.speed.ok ? t.speed.gs_global : 0;
        else if (!strcmp(argv[a], "forbid_desc")) got = t.forbid.ok ? t.forbid.desc : 0;
        else if (!strcmp(argv[a], "fields")) got = t.fields != NULL;
        else if (!strcmp(argv[a], "stage2_on")) got = t.cap[CAP_STAGE2_COMMON].value;
        else if (!strcmp(argv[a], "valid_slot")) got = t.st2.common.valid_slot;
        else if (!strcmp(argv[a], "exec_flag")) got = t.st2.common.exec_flag_off;
        else if (!strcmp(argv[a], "player_tag")) got = t.st2.common.player_tag_off;
        else if (!strcmp(argv[a], "line_factories")) got = t.st2.common.line_factories_off;
        else if (!strcmp(argv[a], "region_id")) got = t.st2.common.region_id_off;
        else if (!strcmp(argv[a], "regions_arr")) got = t.st2.common.regions_arr_off;
        else if (!strcmp(argv[a], "regions_cnt")) got = t.st2.common.regions_cnt_off;
        else if (!strcmp(argv[a], "ti_orders_group")) got = t.st2.common.ti_orders_group;
        /* engineer 73: the battle-plan order tree facts */
        else if (!strcmp(argv[a], "orders_ok")) got = (uint64_t)t.st2.orders.ok;
        else if (!strcmp(argv[a], "orders_vt")) got = t.st2.orders.vt_order;
        else if (!strcmp(argv[a], "orders_lookup")) got = t.st2.orders.lookup_fn;
        else if (!strcmp(argv[a], "order_id_off")) got = t.st2.orders.id_off;
        else if (!strcmp(argv[a], "order_div_get_slot")) got = t.st2.orders.div_get_slot;
        else if (!strcmp(argv[a], "order_div_alive")) got = t.st2.orders.div_alive_off;
        else if (!strcmp(argv[a], "order_div_sub")) got = t.st2.orders.div_sub_off;
        else if (!strcmp(argv[a], "order_provs_off")) got = t.st2.orders.provs_off;
        else if (!strcmp(argv[a], "order_divs_off")) got = t.st2.orders.divs_off;
        else if (!strcmp(argv[a], "order_children_off")) got = t.st2.orders.child_off[0];
        else if (!strcmp(argv[a], "group_lists_off")) got = t.st2.orders.list_off[0];
        /* engineer 74: the fronts a plan root binds to; the array allocator; a division as Assign's check reads it */
        else if (!strcmp(argv[a], "fronts_ok")) got = (uint64_t)t.st2.fronts.ok;
        else if (!strcmp(argv[a], "group_parent_off")) got = t.st2.fronts.group_parent_off;
        else if (!strcmp(argv[a], "theatre_fronts_off")) got = t.st2.fronts.theatre_fronts_off;
        else if (!strcmp(argv[a], "front_sections_off")) got = t.st2.fronts.front_sections_off;
        else if (!strcmp(argv[a], "section_provs_off")) got = t.st2.fronts.section_provs_off;
        else if (!strcmp(argv[a], "province_id_off")) got = t.st2.fronts.province_id_off;
        else if (!strcmp(argv[a], "root_front_off")) got = t.st2.fronts.root_front_off;
        else if (!strcmp(argv[a], "root_range_off")) got = t.st2.fronts.root_range_off;
        else if (!strcmp(argv[a], "alloc_slot")) got = t.st2.common.alloc_slot;
        else if (!strcmp(argv[a], "unit_group_off")) got = t.st2.common.unit_group_off;
        else if (!strcmp(argv[a], "unit_hint")) got = t.st2.common.unit_hint;
        else if (!strcmp(argv[a], "unit_theatre_off")) got = t.st2.common.unit_theatre_off;
        else if (!strcmp(argv[a], "group_parent_ptr_off")) got = t.st2.common.group_parent_ptr_off;
        /* engineer 78: diplomacy */
        else if (!strcmp(argv[a], "diplo_ok")) { got = (uint64_t)t.st2.diplo.ok; fprintf(stderr, "diplo: %s\n", t.st2.diplo.evidence); }
        else if (!strcmp(argv[a], "diplo_war_ok")) got = (uint64_t)t.st2.diplo.war_ok;
        else if (!strcmp(argv[a], "diplo_make_cmd")) got = t.st2.diplo.make_cmd;
        else if (!strcmp(argv[a], "diplo_date_off")) got = t.st2.diplo.date_off;
        else if (!strcmp(argv[a], "diplo_act_date_off")) got = t.st2.diplo.act_date_off;
        else if (!strcmp(argv[a], "diplo_date_vt")) got = t.st2.diplo.date_vt;
        else if (!strcmp(argv[a], "diplo_country_of")) got = t.st2.diplo.country_of;
        else if (!strcmp(argv[a], "wg_holder_off")) got = t.st2.diplo.wg_holder_off;
        else if (!strcmp(argv[a], "wg_list_off")) got = t.st2.diplo.wg_list_off;
        else if (!strcmp(argv[a], "wg_count_off")) got = t.st2.diplo.wg_count_off;
        else if (!strcmp(argv[a], "wg_target_off")) got = t.st2.diplo.wg_target_off;
        else if (!strncmp(argv[a], "act_size:", 9)) { const cmd_type *x = stage2_find(&t.st2, argv[a] + 9); got = x ? x->act_size : 0; }
        else if (!strcmp(argv[a], "diplo_classes")) got = (uint64_t)t.st2.ndcat;
        else if (!strcmp(argv[a], "diplo_buildable")) got = (uint64_t)t.st2.ndyn;
        /* engineer 125: the war goal type database (list, count, token offsets packed one byte each) */
        else if (!strcmp(argv[a], "wgt_offs")) got = t.st2.diplo.wgt_got ? ((uint64_t)t.st2.diplo.wgt_arr_off << 16) | ((uint64_t)t.st2.diplo.wgt_cnt_off << 8) | t.st2.diplo.wgt_tok_off : 0;
        else if (!strncmp(argv[a], "act_type:", 9)) { const cmd_type *x = stage2_find(&t.st2, argv[a] + 9); got = x ? x->act_type : 0; }
        /* engineer 118: a volunteer action's division list, packed as list offset, inline buffer, inline places */
        else if (!strncmp(argv[a], "act_units:", 10)) {
            const cmd_type *x = stage2_find(&t.st2, argv[a] + 10);
            got = x ? ((uint64_t)x->act_units_off << 16) | ((uint64_t)x->act_units_inline << 8) | x->act_units_cap : 0;
        }
        else if (!strncmp(argv[a], "act_goal_off:", 13)) { const cmd_type *x = stage2_find(&t.st2, argv[a] + 13); got = x ? x->act_goal_off : 0; }
        /* engineer 105: a unit action's facts (move.division): its province constructor, and its offsets packed as
         * handle, path data, path count, second array, first flag byte (one byte each, high to low) */
        else if (!strncmp(argv[a], "act_ctor:", 9)) { const cmd_type *x = stage2_find(&t.st2, argv[a] + 9); got = x ? x->act_ctor : 0; }
        else if (!strncmp(argv[a], "act_offs:", 9)) {
            const cmd_type *x = stage2_find(&t.st2, argv[a] + 9);
            got = x ? ((uint64_t)x->act_handle_off << 32) | ((uint64_t)x->act_path_off << 24) | ((uint64_t)x->act_path_cnt_off << 16) |
                      ((uint64_t)x->act_path2_off << 8) | x->act_flag_off : 0;
        }
        /* engineer 79: the area-defence order */
        /* engineer 82: the building reference */
        else if (!strcmp(argv[a], "build_ok")) { got = (uint64_t)t.st2.build.ok; fprintf(stderr, "construction: %s\n", t.st2.build.evidence); }
        else if (!strcmp(argv[a], "build_tmpl")) got = ((uint64_t)t.st2.build.tmpl_real_off << 32) | ((uint64_t)t.st2.build.tmpl_prov_off << 8) | t.st2.build.tmpl_id_off;
        else if (!strcmp(argv[a], "build_loc")) got = ((uint64_t)t.st2.build.state_id_off << 16) | t.st2.build.prov_id_off;
        else if (!strcmp(argv[a], "build_ref_vt")) got = t.st2.build.ref_vt;
        else if (!strcmp(argv[a], "train_ok")) { got = (uint64_t)t.st2.train.ok; fprintf(stderr, "training: %s\n", t.st2.train.evidence); }
        else if (!strcmp(argv[a], "train_can")) got = t.st2.train.can_train;
        else if (!strcmp(argv[a], "train_offs")) got = ((uint64_t)t.st2.train.deploy_off << 16) | t.st2.train.tmpl_body_off;
        /* engineer 84: the organisation facts */
        else if (!strcmp(argv[a], "mio_ok")) { got = (uint64_t)t.st2.mio.ok; fprintf(stderr, "mio: %s\n", t.st2.mio.evidence); }
        else if (!strcmp(argv[a], "mio_list")) got = ((uint64_t)t.st2.mio.mgr_off << 32) | ((uint64_t)t.st2.mio.orgs_off << 16) | t.st2.mio.orgs_cnt;
        else if (!strcmp(argv[a], "mio_pol")) got = ((uint64_t)t.st2.mio.pol_off << 16) | t.st2.mio.pol_cnt;
        else if (!strcmp(argv[a], "mio_org")) got = ((uint64_t)t.st2.mio.tag_off << 32) | ((uint64_t)t.st2.mio.key_off << 16) | t.st2.mio.policy_off;
        else if (!strcmp(argv[a], "mio_scalars")) got = ((uint64_t)t.st2.mio.funds_off << 32) | ((uint64_t)t.st2.mio.size_off << 16) | t.st2.mio.points_off;
        else if (!strcmp(argv[a], "mio_traits")) got = ((uint64_t)t.st2.mio.unlocked_off << 48) | ((uint64_t)t.st2.mio.queue_off << 32) | ((uint64_t)t.st2.mio.queue_cnt << 16) | t.st2.mio.tree_off;
        else if (!strcmp(argv[a], "mio_tree")) got = ((uint64_t)t.st2.mio.tree_arr << 32) | ((uint64_t)t.st2.mio.tree_cnt << 16) | t.st2.mio.trait_stride;
        else if (!strcmp(argv[a], "mio_gate")) got = ((uint64_t)t.st2.mio.gate_policy_off << 32) | ((uint64_t)t.st2.mio.gate_org_off << 16) | t.st2.mio.line_org_off;
        else if (!strcmp(argv[a], "mio_fns")) got = t.st2.mio.policy_gate;
        /* engineer 127: the leader trait database (CLearnTraitCommand's reader) */
        else if (!strcmp(argv[a], "trait_got")) got = t.st2.trait_got;
        else if (!strcmp(argv[a], "trait_offs")) got = ((uint64_t)t.st2.trait_arr_off << 32) | ((uint64_t)t.st2.trait_cnt_off << 16) | t.st2.trait_tok_off;
        /* engineer 131: the faction goal database (CAddFactionGoalCommand's reader) and the faction's goal list */
        else if (!strcmp(argv[a], "fg_got")) { const cmd_type *fg = stage2_find(&t.st2, "faction.goal"); got = t.st2.fg_got; fprintf(stderr, "faction.goal: %s\n", fg ? fg->evidence : "?"); }
        else if (!strcmp(argv[a], "fg_hash")) got = ((uint64_t)t.st2.fg_data_off << 48) | ((uint64_t)t.st2.fg_mask_off << 40) | ((uint64_t)t.st2.fg_probe_off << 32) |
                                                 ((uint64_t)t.st2.fg_stride << 24) | ((uint64_t)t.st2.fg_dist_off << 16) | ((uint64_t)t.st2.fg_tok_off << 8) | t.st2.fg_val_off;
        else if (!strcmp(argv[a], "fg_list")) got = ((uint64_t)t.st2.fg_holder_off << 48) | ((uint64_t)t.st2.fg_faction_off << 32) | ((uint64_t)t.st2.fg_goals_off << 16) | t.st2.fg_goals_cnt_off;
        else if (!strcmp(argv[a], "fg_stride")) got = t.st2.fg_goal_stride;
        /* engineer 133: the continuous focus database (CSetContinuousFocusCommand's reader) */
        else if (!strcmp(argv[a], "cf_got")) { const cmd_type *cf = stage2_find(&t.st2, "cfocus.set"); got = t.st2.cf_got; fprintf(stderr, "cfocus.set: %s\n", cf ? cf->evidence : "?"); }
        else if (!strcmp(argv[a], "cf_offs")) got = ((uint64_t)t.st2.cf_arr_off << 24) | ((uint64_t)t.st2.cf_cnt_off << 16) | ((uint64_t)t.st2.cf_hash_off << 8) | t.st2.cf_name_off;
        else if (!strcmp(argv[a], "holds_ok")) { got = (uint64_t)t.st2.holds.ok; fprintf(stderr, "holds: %s\n", t.st2.holds.evidence); }
        else if (!strcmp(argv[a], "holds_kind")) got = t.st2.holds.kind_area;
        else if (!strcmp(argv[a], "holds_area_off")) got = t.st2.holds.area_off;
        else if (!strcmp(argv[a], "holds_area_cnt")) got = t.st2.holds.area_cnt;
        else if (!strcmp(argv[a], "holds_setting_off")) got = t.st2.holds.setting_off;
        /* engineer 76: the event window and the pending-event records */
        else if (!strcmp(argv[a], "events_ok")) got = (uint64_t)t.st2.events.ok;
        else if (!strcmp(argv[a], "events_vt_window")) got = t.st2.events.vt_window;
        else if (!strcmp(argv[a], "events_avail")) got = t.st2.events.avail_fn;
        else if (!strcmp(argv[a], "win_event_off")) got = t.st2.events.win_event_off;
        else if (!strcmp(argv[a], "win_open_off")) got = t.st2.events.win_open_off;
        else if (!strcmp(argv[a], "win_flag_off")) got = t.st2.events.win_flag_off;
        else if (!strcmp(argv[a], "ev_opts_off")) got = t.st2.events.ev_opts_off;
        else if (!strcmp(argv[a], "opt_index_off")) got = t.st2.events.opt_index_off;
        else if (!strcmp(argv[a], "pend_arr_off")) got = t.st2.events.pend_arr_off;
        else if (!strncmp(argv[a], "ctor:", 5) || !strncmp(argv[a], "on:", 3) || !strncmp(argv[a], "type:", 5) ||
                 !strncmp(argv[a], "conf:", 5)) {
            /* per stage-2 op (engineer 71): ctor:OP = the UI constructor found, on:OP = 1 when every check resolved,
             * type:OP = the protocol type id; conf:OP (engineer 85) = the confidence bits (C_* in cmd2.h) */
            const cmd_type *x = stage2_find(&t.st2, strchr(argv[a], ':') + 1);
            found = x != NULL;
            if (x) got = !strncmp(argv[a], "conf:", 5) ? x->conf : argv[a][0] == 'c' ? x->ctor : argv[a][0] == 'o' ? (uint64_t)x->ok : x->type_id;
        }
        else {
            found = 0;
            for (int i = 0; i < CAP_COUNT; i++)
                if (!strcmp(argv[a], t.cap[i].name)) { got = t.cap[i].value; found = 1; }
        }
        int match = found && got == want;
        bad += !match;
        printf("expect %-18s 0x%llx got 0x%llx %s\n", argv[a], (unsigned long long)want, (unsigned long long)got,
               match ? "MATCH" : "MISMATCH");
    }
    free(g); free(r); scan_image_free(&im);
    return bad ? 1 : 0;
}
