#!/bin/bash

set -e

mkdir -p data/system/maps/

# Need this first
if [ ! -f data/input/popdat.bin ]; then
	# We probably don't have this map yet.
	if [ ! -f data/system/maps/huge_seattle.bin ]; then
		cd precompute;
		RUST_BACKTRACE=1 cargo run --release ../data/input/raw_maps/huge_seattle.bin --disable_psrc_scenarios;
		cd ..;
	fi

	cd popdat;
	cargo run --release;
	cd ..;
fi

release_mode=""
psrc_scenarios=""
no_fixes=""
for arg in "$@"; do
	if [ "$arg" == "--release" ]; then
		release_mode="--release";
	elif [ "$arg" == "--disable_psrc_scenarios" ]; then
		psrc_scenarios="--disable_psrc_scenarios";
	elif [ "$arg" == "--nofixes" ]; then
		no_fixes="--nofixes";
	else
		# Just recompute a single map.
		cd precompute;
		RUST_BACKTRACE=1 cargo run $release_mode ../data/input/raw_maps/$arg.bin $psrc_scenarios $no_fixes;
		cd ..;
		exit;
	fi
done

for map_path in `ls data/input/raw_maps/`; do
	map=`basename $map_path .bin`;
	echo "Precomputing $map";
	cd precompute;
	RUST_BACKTRACE=1 cargo run $release_mode ../data/input/raw_maps/$map.bin $psrc_scenarios $no_fixes;
	cd ..;
done
