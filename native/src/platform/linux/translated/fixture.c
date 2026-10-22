/* Generated offline FEX ownership fixture. Build on x86 Linux:
 * cc -O2 -fPIE -pie -Wl,--build-id -o translated-fixture fixture.c
 * No game, process discovery, display or input. Parent owns this child only.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/random.h>
#include <sys/syscall.h>
#include <unistd.h>
extern char **environ;
static unsigned char nonce[16];
#define FD_GENERATION_MARKER "HOI4_FEX_FIXTURE_FD_GENERATION"
int main(int argc, char **argv) {
    const char *marker=getenv(FD_GENERATION_MARKER);
    int fd_generation=marker && !strcmp(marker,"1");
    int quit_only=argc==0 && fd_generation;
    /* Only our immediately preceding FD exec may enter the observed FEX argc0
     * generation. Consume its marker; it never authorizes another exec. */
    if (unsetenv(FD_GENERATION_MARKER)) return 8;
    errno=0;
    ssize_t random_result=getrandom(nonce, sizeof(nonce), 0);
    int random_errno=errno;
    fprintf(stderr,"fixture_start argc=%d getrandom_result=%lld getrandom_errno=%d fd_generation=%d quit_only=%d\n",argc,(long long)random_result,random_errno,fd_generation,quit_only);
    if (argc != 1 && !quit_only) {
        fprintf(stderr,"fixture_invalid_argc argc=%d argv0_present=%d argv0='%.128s' argv1_present=%d argv1='%.128s'\n",
                argc,argc>0 && argv && argv[0],argc>0 && argv && argv[0]?argv[0]:"",
                argc>1 && argv && argv[1],argc>1 && argv && argv[1]?argv[1]:"");
        return 2;
    }
    if (quit_only) fprintf(stderr,"fixture_fex_fd_argv_limitation argc=0 marked_fd_generation=1 quit_only=1\n");
    if (random_result != (ssize_t)sizeof(nonce)) return 2;
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("{\"ready\":true,\"argc\":%d,\"quit_only\":%s,\"address\":%llu,\"nonce\":\"",argc,quit_only?"true":"false", (unsigned long long)(uintptr_t)nonce);
    for (size_t i=0;i<sizeof(nonce);++i) printf("%02x",nonce[i]);
    puts("\"}");
    char command[64];
    while (fgets(command,sizeof(command),stdin)) {
        if (quit_only) {
            if (!strcmp(command,"quit\n")) return 0;
            fprintf(stderr,"fixture_quit_only_refusal argc=0\n");
            return 7;
        }
        if (!strcmp(command,"selfexec\n")) {
            execve(argv[0],argv,environ);
            perror("selfexec"); return 3;
        }
        if (!strcmp(command,"execveat\n")) {
            int fd=open(argv[0],O_RDONLY|O_CLOEXEC);
            if (fd<0) return 4;
            fprintf(stderr,"fixture_execveat argc=%d argv0='%.128s' fd=%d flags=AT_EMPTY_PATH\n",argc,argv[0],fd);
            if (setenv(FD_GENERATION_MARKER,"1",1)) return 8;
            syscall(SYS_execveat,fd,"",argv,environ,AT_EMPTY_PATH);
            perror("execveat"); return 5;
        }
        if (!strcmp(command,"quit\n")) return 0;
        return 6;
    }
    return 0;
}
