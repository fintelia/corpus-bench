#define WUFFS_IMPLEMENTATION
#define WUFFS_CONFIG__STATIC_FUNCTIONS
#define WUFFS_CONFIG__ENABLE_DROP_IN_REPLACEMENT__STB

#include "stdio.h"
#include "wuffs-v0.4.c"

unsigned char *wuffs_load_from_memory(const unsigned char *buffer,
                                 int len,
                                 int *x,
                                 int *y,
                                 int *channels_in_file,
                                 int desired_channels)
{
    return stbi_load_from_memory(
        buffer, len, x, y, channels_in_file, desired_channels);
}