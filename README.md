# Blockcheck Wrapper

**64 стратегии за 2.4 секунды** вместо 72 — ускорение в ~30x.

**blockcheckw** — параллельный сканер стратегий обхода DPI. Переписан с `bash` на `Rust`.
Оригинальный `blockcheck2.sh` проверяет стратегии последовательно — одну за другой.
`blockcheckw` запускает их параллельно, изолируя воркеры через выделенные диапазоны портов.

## Производительность

Результаты нагрузочного теста — 64 стратегии `--dpi-desync=fake` с TTL 1..64:

| Workers | Время | Throughput | Ускорение |
|--------:|------:|-----------:|----------:|
| 1       | 72.1s |  0.9/sec   |     1.0x  |
| 2       | 36.8s |  1.7/sec   |     2.0x  |
| 4       | 19.5s |  3.3/sec   |     3.7x  |
| 8       | 10.4s |  6.2/sec   |     6.9x  |
| 16      |  6.1s | 10.5/sec   |    11.8x  |
| 32      |  3.5s | 18.3/sec   |    20.6x  |
| **64**  |**2.4s**|**27.1/sec**| **30.5x** |
| 128     |  2.8s | 22.7/sec   |    25.6x  |

Масштабирование почти линейное до 64 воркеров. На 128 начинается overhead от количества nfqws2-процессов.
0 ошибок на всех масштабах. Все nfqws2-процессы и nftables-правила корректно очищаются.

### Тестовый стенд

- **Роутер**: FriendlyElec NanoPi R3S
- **CPU**: 4x ARM Cortex-A53
- **RAM**: 2 GB
- **OS**: OpenWrt 25.12, kernel 6.12
- **Бинарник**: статически слинкованный `aarch64-unknown-linux-musl`, 2.6 MB

## Как это работает

Каждый воркер получает изолированный слот:
- Уникальный диапазон source-портов (sport) для curl `--local-port`
- Персональное nftables-правило, матчащее только его sport-диапазон
- Свой экземпляр nfqws2 на выделенной NFQUEUE

Это позволяет запускать десятки стратегий одновременно без конфликтов.

## Сборка

```shell
cargo build --release
```

Кросс-компиляция для роутера (aarch64 + OpenWrt/musl):
```shell
cargo build --release --target aarch64-unknown-linux-musl
scp target/aarch64-unknown-linux-musl/release/blockcheckw root@router:/tmp/
```

## Тесты

### Unit-тесты (парсинг handle, CurlVerdict, WorkerSlot)
```shell
cargo test --lib
```
### Нагрузочный тест масштабирования на роутере
```shell
ssh root@router "/tmp/blockcheckw --benchmark"
```
#### Определение оптимального количества workers(FIXME)
```shell
blockcheckw -w 32 benchmark -s 64 --scaling
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
