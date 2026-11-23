/* HOI4 Live Tools in-process module: the scanner core (stage 0, read-only).
 *
 * Shared by the module (which scans its own process image) and the offline tester (which scans the executable
 * file). Nothing here calls the game, writes memory or touches the OS: it reads an image view and returns
 * addresses with the evidence that found them. Every lookup is bounds-checked against the image's segments, so a
 * failed or odd scan yields "missing", never a wild read.
 *
 * Format: Mach-O x86_64 (macOS, Itanium C++ ABI with libc++). The same capability names are planned for the ELF
 * (Linux, Itanium ABI, libstdc++) and PE (Windows, MSVC ABI) image views; see the feasibility study. */
#ifndef HOI4_INPROC_SCAN_H
#define HOI4_INPROC_SCAN_H
#include <stddef.h>
#include <stdint.h>

#define SCAN_MAX_SEGS 16

typedef struct {
    char name[17];
    uint64_t vmaddr, vmsize, fileoff, filesize;
} scan_seg;

typedef struct {
    /* Readable bytes of the image. For a file view: the mapped file (base) and translate by segment fileoff.
     * For a process view: memory, translated by vmaddr + slide. */
    const uint8_t *file;   /* non-NULL for a file view */
    size_t file_size;
    int64_t slide;         /* process view: _dyld_get_image_vmaddr_slide; file view: 0 */
    scan_seg segs[SCAN_MAX_SEGS];
    int nsegs;
    char uuid[37];
    /* sections used by the scans (unslid vm addresses) */
    uint64_t text_addr, text_size;         /* __TEXT,__text */
    uint64_t cstr_addr, cstr_size;         /* __TEXT,__cstring */
    uint64_t tconst_addr, tconst_size;     /* __TEXT,__const (type-info names on this toolchain) */
    uint64_t dconst_addr, dconst_size;     /* __DATA,__const (type info objects, vtables) */
    uint64_t data_lo, data_hi;             /* whole __DATA segment (globals incl. zero-fill) */
    uint32_t symoff, nsyms, stroff, strsize;
    uint32_t fstarts_off, fstarts_size;
    uint64_t *fstarts; size_t nfstarts;    /* function starts, ascending (heap; scan_image_free) */
} scan_image;

/* One resolved capability: a name, an address (unslid image address unless said), a status and its evidence. */
typedef struct {
    const char *name;
    int ok;
    uint64_t value;
    uint64_t extra;       /* e.g. vtable slot, type id, object size */
    char source[24];      /* "scan", "export", "rtti", "table" */
    char evidence[160];
} scan_cap;

/* Open a view. header = the Mach-O header in memory (process view) or the mapped file (file view). */
int scan_image_open_file(scan_image *im, const uint8_t *file, size_t size);
int scan_image_open_process(scan_image *im, const void *header, int64_t slide);
void scan_image_free(scan_image *im);

/* Bounded access to file-backed bytes at an unslid vm address; NULL if any byte is outside a segment's file part. */
const uint8_t *scan_ptr(const scan_image *im, uint64_t vmaddr, size_t len);
int scan_q(const scan_image *im, uint64_t vmaddr, uint64_t *out);

/* The resolutions the stage-0 module performs. Each fills *cap and returns cap->ok. */
int scan_post_function(const scan_image *im, scan_cap *cap);                 /* CCommandQueue::Post */
int scan_queue_root(const scan_image *im, uint64_t post, scan_cap *cap);     /* idler global + queue getter slot */
int scan_command_factory(const scan_image *im, const char *cls, scan_cap *factory, scan_cap *vtable,
                         scan_cap *type_id);                                 /* CCommandHelper<cls>::...::Create */
int scan_class_vtable(const scan_image *im, const char *mangled_name, scan_cap *cap); /* RTTI name -> vtable */
/* Globals in __DATA referenced by at least min_refs RIP-relative loads, most referenced first (heap array). */
size_t scan_global_candidates(const scan_image *im, unsigned min_refs, uint64_t **out, unsigned **refs);

/* JSON text of a capability (no allocation; buf >= 512). */
const char *scan_cap_json(const scan_cap *cap, char *buf, size_t n);
#endif
