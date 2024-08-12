set -e

binpath="target/release/sand"

if [ ! -f $binpath ]; then
    echo 'Please build the project first with `cargo build --release`'
    exit 1
fi

# git describe doesn't work by default in CI, so we use an action for it.
if [ -z "$GIT_DESCRIBE" ]; then
    echo "Not in CI, running git describe"
    version="$(git describe)"
else
    echo "In CI, getting version from GIT_DESCRIBE"
    version=$GIT_DESCRIBE
fi


dir="sand-$version"

set -x

mkdir -p "release/$dir"

cp -f $binpath release/$dir/
cp -f resources/systemd/sand.service release/$dir/
cp -f resources/systemd/sand.socket release/$dir/
cp -f resources/timer_sound.opus release/$dir/
cp -f scripts/install_release.sh release/$dir/
cp -f LICENSE release/$dir/
cp -f README.md release/$dir/

pushd release/ > /dev/null

strip $dir/sand

archive="$dir-x86_64-linux.tar.zst"

tar --zstd -cvf $archive $dir 2>&1 > /dev/null

popd > /dev/null

set +x

echo "release created at release/$dir"
echo "release archive created at release/$archive"
