@echo off
cargo build --examples

echo Run deletes-itself.exe
target\debug\examples\deletes-itself.exe
if not exist target\debug\examples\deletes-itself.exe (
    echo   deletes-itself.exe was successfully deleted
)

echo.
echo Run hello.exe
target\debug\examples\hello.exe

echo.
echo Run replaces-itself.exe
target\debug\examples\replaces-itself.exe
echo Run replaces-itself.exe
target\debug\examples\replaces-itself.exe

echo.
echo Run replaces-itself-with.exe
target\debug\examples\replaces-itself-with.exe
echo Run replaces-itself-with.exe
target\debug\examples\replaces-itself-with.exe

echo.
echo Run deletes-itself-outside-path.exe
target\debug\examples\deletes-itself-outside-path.exe
if not exist target\debug\examples\NUL (
    echo   built examples were successfully deleted
)