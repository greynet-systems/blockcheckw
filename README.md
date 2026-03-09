# blockcheckw

`blockcheck.sh` wrapper for better scanning speed — переписан с Kotlin на Rust.

## Что делает

Параллельно тестирует стратегии обхода DPI через nfqws2, используя:
- **nftables** — per-worker правила с sport-изоляцией
- **curl** — тестирование HTTP/HTTPS доступности
- **nfqws2** — десинхронизация DPI

## Архитектура

```
src/
├── main.rs                  # Точка входа + signal handler
├── lib.rs                   # Модули
├── error.rs                 # BlockcheckError, TaskResult
├── config.rs                # CoreConfig, Protocol, константы
├── system/
│   └── process.rs           # run_process, BackgroundProcess
├── firewall/
│   └── nftables.rs          # prepare_table, add_worker_rule, remove_rule, drop_table
├── network/
│   └── curl.rs              # curl_test_http/tls12/tls13, CurlVerdict
├── worker/
│   ├── nfqws2.rs            # start_nfqws2, detect_nfqws2_path
│   └── slot.rs              # WorkerSlot с sport-изоляцией
└── pipeline/
    ├── worker_task.rs        # execute_worker_task — полный цикл одной стратегии
    └── runner.rs             # run_parallel — батч-оркестратор с JoinSet
tests/
└── parallel_bench.rs        # Нагрузочный тест масштабирования
```

## Сборка

```shell
cargo build
```

## Тесты

```shell
# Unit-тесты (парсинг handle, CurlVerdict, WorkerSlot)
cargo test --lib
```
```shell
# Нагрузочный тест (требует root + nfqws2 + nftables)
sudo cargo test --test parallel_bench -- --nocapture
```

## Зависимости

- Rust (edition 2021)
- tokio (async runtime)
- nfqws2 в `/opt/zapret2/`
- nftables
- curl

## Обновить git submodule

```shell
git submodule update --remote reference/zapret2
```
