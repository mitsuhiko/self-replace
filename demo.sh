cargo build --examples

echo "Run deletes-itself.exe"
target/debug/examples/deletes-itself

if [ ! -f target/debug/examples/deletes-itself ]; then
  echo "  deletes-itself.exe was successfully deleted"
fi

echo
echo "Run hello.exe"
target/debug/examples/hello

echo
echo "Run replaces-itself"
target/debug/examples/replaces-itself
echo "Run replaces-itself"
target/debug/examples/replaces-itself

echo
echo "Run deletes-itself-outside-path"
target/debug/examples/deletes-itself-outside-path

if [ ! -d target/debug/examples ]; then
  echo "  built exampels were successfully deleted"
fi