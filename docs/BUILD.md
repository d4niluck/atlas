# Сборка Atlas

## macOS: локальная сборка на Apple Silicon

Скрипт `ui/src-tauri/package-macos.sh` собирает приложение из текущего коммита Git (`HEAD`); незакоммиченные изменения не входят в сборку. Модели в приложение не включаются. Скрипту требуются macOS на Apple Silicon, Git, Xcode Command Line Tools, Node.js/npm, Rust/Cargo, локальный Cargo cache и заранее подготовленные зависимости:

- каталог runtime llama с `llama-server`, его библиотеками и лицензией;
- каталог PDFium с `lib/libpdfium.dylib`, `include/fpdfview.h`, лицензией и подкаталогом `licenses`;
- каталог `ui/node_modules`, совместимый с lock-файлом проекта.

```sh
ui/src-tauri/package-macos.sh \
  /path/to/atlas /path/to/llama /path/to/pdfium \
  /path/to/node_modules /path/to/Atlas.app
```

Для повторной сборки можно добавить два существующих каталога с кэшем Cargo `target`: сначала для backend, затем для приложения. Скрипт выполняет `cargo build --locked --offline`; без предварительно загруженных зависимостей он завершится ошибкой. Он не подготавливает и не проверяет происхождение внешних библиотек и не заменяет уже существующий выходной файл.

Пакет подписан локальной подписью ad hoc. Это не подпись Apple Developer и не нотариализация: открытие на другом Mac может потребовать дополнительных действий. Сборка на чистом Mac и на Intel Mac не проверена.

## Linux и Windows

Для Ubuntu x86-64 подготовлен [прототип сборки через Docker](../scripts/linux-release/README.md); область проверок и ограничения указаны в его инструкции. Готовой версии Windows пока нет: требуется перенос платформенно-зависимых компонентов и проверка на Windows.
