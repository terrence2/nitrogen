## Running

We mmap every tile, so don't forget to up the max map count from 64K to something more reasonable.
> sudo sysctl -w vm.max_map_count=1073741824
