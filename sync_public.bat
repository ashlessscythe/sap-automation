@echo off
:: sync_public.bat

echo Ensuring you're on the dev branch...
git checkout dev

:: Optional: Create a tag
set /p TAG="Enter tag name (leave blank to skip): "
if not "%TAG%"=="" (
  git tag -a "%TAG%" -m "Release %TAG%"
  git push --tags
)

echo Syncing public to dev...
git checkout public
git reset --hard dev
git push --force
git checkout dev

echo Public branch synced with dev successfully!
pause
