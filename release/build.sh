#!/bin/bash
# Called by Github Actions workflow

set -e

output=$1;
runner=$2;
binary=$3;

mkdir $output
curl -L -o release_data.zip https://www.dropbox.com/s/wzw7gucvccoxy66/release_data.zip?dl=0 
cd $output
unzip ../release_data.zip
mv release_data data
cd ..

cp docs/INSTRUCTIONS.md $output
cp release/$runner $output
mkdir $output/game
cp $binary $output/game
cp -Rv game/assets $output/game

zip -r $output $output
rm -rf $output release_data.zip
