/* Оракул для TDD фикстур tiano-op: разворачивает payload эталонным
   декодером UEFITool. Спека tiano-op §3. Сборка — в gen-tiano-fixtures.sh. */
#include <stdio.h>
#include <stdlib.h>
#include "EfiTianoDecompress.h"

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <4|5>  (stdin payload, stdout decompressed)\n", argv[0]);
        return 2;
    }
    int version = (argv[1][0] == '5') ? 2 : 1;

    unsigned char *src = NULL;
    size_t len = 0, cap = 0;
    for (;;) {
        if (len == cap) {
            cap = cap ? cap * 2 : 65536;
            unsigned char *tmp = realloc(src, cap);
            if (!tmp) { fprintf(stderr, "oom\n"); return 1; }
            src = tmp;
        }
        size_t n = fread(src + len, 1, cap - len, stdin);
        if (n == 0) break;
        len += n;
    }

    UINT32 dst = 0, scratch = 0;
    if (EfiTianoGetInfo(src, (UINT32)len, &dst, &scratch) != 0) {
        fprintf(stderr, "getinfo failed\n");
        return 1;
    }
    fprintf(stderr, "orig=%u scratch=%u\n", dst, scratch);

    unsigned char *out = malloc(dst ? dst : 1);
    unsigned char *scr = malloc(scratch ? scratch : 1);
    if (!out || !scr) { fprintf(stderr, "oom\n"); return 1; }

    EFI_STATUS st = (version == 2)
        ? TianoDecompress(src, (UINT32)len, out, dst, scr, scratch)
        : EfiDecompress(src, (UINT32)len, out, dst, scr, scratch);
    if (st != 0) { fprintf(stderr, "decompress failed 0x%lx\n", (unsigned long)st); return 1; }

    fwrite(out, 1, dst, stdout);
    return 0;
}
