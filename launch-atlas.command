#!/bin/zsh
# Open an already built application without development tools.
set -eu
atlas_dir=${0:A:h}
for app in "$atlas_dir/Atlas.app" "$HOME/Applications/Atlas.app" "/Applications/Atlas.app"; do
  if [[ -d "$app" ]]; then
    exec /usr/bin/open "$app"
  fi
done
print -u2 'Atlas.app не найден. Установите приложение в папку Applications или положите Atlas.app рядом с этим файлом.'
exit 1
