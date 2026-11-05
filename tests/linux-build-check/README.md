# Linux: проверка компиляции оболочки Atlas

Проверка компилирует только Rust/Tauri-оболочку на Linux. Вместо интерфейса создаётся пустая страница; сервер, модели, FAISS и готовый пакет не собираются, приложение не запускается. Нужен Docker с доступом к Debian и crates.io.

Из корня Atlas запустите команду. В Docker попадут только отслеживаемые файлы оболочки из текущего коммита; незакоммиченные изменения не включаются:

```sh
(
  TASK_ATLAS_CONTEXT=$(mktemp -d)
  trap 'rm -rf "$TASK_ATLAS_CONTEXT"' EXIT
  git archive HEAD ui/src-tauri | tar -x -C "$TASK_ATLAS_CONTEXT"
  mkdir -p "$TASK_ATLAS_CONTEXT/ui/src-tauri/resources"
  git show HEAD:tests/linux-build-check/Dockerfile > "$TASK_ATLAS_CONTEXT/Dockerfile"
  docker build --progress plain "$TASK_ATLAS_CONTEXT"
)
```

На Mac с Apple Silicon проверяется Linux arm64. Это не проверка Linux x86-64, GUI, переноса данных, загрузки моделей или установки.

Версия Rust указана, но образ не закреплён по digest и версии пакетов apt не зафиксированы; результат может измениться со временем. Для повторяемых выпусков нужно отдельно закрепить образ и системные пакеты.
