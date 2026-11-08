# Сборка Atlas

## macOS: локальная сборка на Apple Silicon

Скрипт `ui/src-tauri/package-macos.sh` собирает приложение из текущего коммита Git (`HEAD`); незакоммиченные изменения не входят в сборку. Модели в приложение не включаются. Скрипту требуются macOS на Apple Silicon, Git, CMake, Xcode Command Line Tools, Node.js/npm, Rust/Cargo, локальный Cargo cache и заранее подготовленные зависимости:

- каталог runtime llama с `llama-server`, его библиотеками и лицензией;
- каталог PDFium с `lib/libpdfium.dylib`, `include/fpdfview.h`, лицензией и подкаталогом `licenses`;
- каталог `ui/node_modules`, совместимый с lock-файлом проекта.
- исходные репозитории FAISS v1.15.1 и LLVM/OpenMP llvmorg-21.1.8, проверенные на указанных ниже коммитах.
- собранный каталог whisper.cpp для голосового ввода.

Например, подготовить исходники можно так:

```sh
git clone --depth 1 --branch v1.15.1 https://github.com/facebookresearch/faiss.git faiss
git clone --depth 1 --branch llvmorg-21.1.8 --filter=blob:none --sparse https://github.com/llvm/llvm-project.git llvm-project
git -C llvm-project sparse-checkout set openmp cmake llvm/cmake
```

Сборка проверяет коммиты FAISS `75c755c5a1fc854756aaf3bd1131213aeb21d8d1` и LLVM `2078da43e25a4623cab2d0d60decddf709aaea28`, проверяет отсутствие изменений отслеживаемых исходников и копирует только файлы под Git. FAISS и OpenMP собираются для macOS 14 и включаются в приложение вместе с лицензиями.

Для whisper.cpp подготовьте исходники и соберите их на зафиксированном коммите:

```sh
git clone https://github.com/ggml-org/whisper.cpp.git /path/to/whisper.cpp
git -C /path/to/whisper.cpp checkout f92e1e4d1056700c39d4e86c1140f0a088fe1ab6
scripts/macos/build-whisper.sh /path/to/whisper.cpp /path/to/whisper-build
mkdir -p /path/to/native-target /path/to/app-target
```

```sh
ui/src-tauri/package-macos.sh \
  /path/to/atlas /path/to/llama /path/to/pdfium \
  /path/to/node_modules /path/to/Atlas.app /path/to/faiss /path/to/llvm-project \
  /path/to/native-target /path/to/app-target /path/to/whisper-build/bin
```

Каталоги `target` сохраняют кэш Cargo: сначала для backend, затем для приложения. Скрипт выполняет `cargo build --locked --offline`; без предварительно загруженных зависимостей он завершится ошибкой. Он не подготавливает и не проверяет происхождение внешних библиотек и не заменяет уже существующий выходной файл. Модели речи загружаются отдельно и хранятся вне приложения.

Для запуска или тестирования backend напрямую сначала соберите библиотеки в игнорируемую Git папку (она не должна существовать):

```sh
scripts/macos/build-faiss.sh /path/to/faiss /path/to/llvm-project native/atlas-faiss-rust/vendor/lib
DYLD_LIBRARY_PATH="$PWD/native/atlas-faiss-rust/vendor/lib" cargo test --manifest-path native/atlas-faiss-rust/Cargo.toml
```

Для прямого запуска backend укажите тот же `DYLD_LIBRARY_PATH`, чтобы macOS нашла FAISS; в упакованном приложении библиотеки находятся рядом с backend.

Проверка сценария сборки: `python3 tests/macos/test_build_faiss.py /path/to/faiss /path/to/llvm-project`.

Пакет подписан локальной подписью ad hoc. Это не подпись Apple Developer и не нотариализация: открытие на другом Mac может потребовать дополнительных действий. Сборка на чистом Mac и на Intel Mac не проверена.

## Linux и Windows

Для Ubuntu x86-64 подготовлен [прототип сборки через Docker](../scripts/linux-release/README.md); область проверок и ограничения указаны в его инструкции. Готовой версии Windows пока нет: требуется перенос платформенно-зависимых компонентов и проверка на Windows.
