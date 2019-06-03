[![Build Status](https://travis-ci.org/lialsoftlab/trlogic_test.svg?branch=master)](https://travis-ci.org/lialsoftlab/trlogic_test)
[![Coverage Status](https://coveralls.io/repos/github/lialsoftlab/trlogic_test/badge.svg?branch=master)](https://coveralls.io/github/lialsoftlab/trlogic_test?branch=master)

# trlogic_test

Установка
---------
1. Непосредственная сборка и установка на локальной машине:

```bash
$ git clone https://github.com/lialsoftlab/trlogic_test \
  && cd trlogic_test \
  && cargo test --all \
  && cargo run --release
```

Микросервис в таком случае подключится и будет отвечать на http://localhost:8000, что можно изменить посредством ключей `--host` и `--port` файлы будут сохранятся в ./uploads/ можно изменить ключем `--upload`. Установкой переменной среды RUST_LOG=trlogic_test=info (или debug или trace) можно управлять журналированием запросов.

2. Сборка и запуск через docker-compose:

```bash
$ git clone https://github.com/lialsoftlab/trlogic_test \
  && cd trlogic_test \
  && docker-compose up
```

Микросервис будет собран, развернут и запущен на дефолтном экземпляре докера на 8000 порту.

Работа с микросервисом
----------------------

Микросервис отвечает на POST запросы по url `/images` с `Content-Type: multipart/form-data`, в таком случае в файлы будут сохранены все поля имеющие тип image/..., либо с `Content-Type: application/json` в таком случае в теле запроса должен быть размещен JSON-массив с объектами описывающими ссылки на внешние ресурсы которые нужно скачать, либо данные картинки в формате base64.

Пример допустимого запроса:

```javascript
[
    { 
        "url": "https://via.placeholder.com/123.jpg"
    },

    {
        "filename": "sample.jpg",
        "content_type": "image/jpeg",
        "data": "~~~base64-encoded-image-data-here~~~"
    }
]
```
