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
cargo build --release --target aarch64-unknown-linux-musl &&
scp target/aarch64-unknown-linux-musl/release/blockcheckw root@router:/tmp/
```

## Тесты

### Unit-тесты (парсинг handle, CurlVerdict, WorkerSlot)
```shell
cargo test --lib
```
### Бенчмарк: автоопределение оптимального числа воркеров

```shell
blockcheckw benchmark
```

Автоматически тестирует степени двойки (1, 2, 4, ...) до `CPU * 16` и выдаёт готовую рекомендацию:

```
=== blockcheckw benchmark ===
domain=rutracker.org  protocol=HTTP  strategies=64  max_workers=128

 Workers  Elapsed(s)  Throughput  Speedup  Errors
 -------  ----------  ----------  -------  ------
      1*        8.85       0.9/s     1.0x       0
       2       36.80       1.7/s     1.9x       0
       4       19.50       3.3/s     3.7x       0
       8       10.40       6.2/s     6.9x       0
      16        6.10      10.5/s    11.7x       0
      32        3.50      18.3/s    20.3x       0
      64        2.40      27.1/s    30.1x       0
     128        2.80      22.7/s    25.2x       0
  * baseline probe: 8 strategies (I/O-bound, throughput stable)

Recommended: blockcheckw -w 64
```

**Как читать таблицу:**

| Колонка      | Значение                                                       |
|:-------------|:---------------------------------------------------------------|
| `Workers`    | Число параллельных воркеров в этом прогоне                     |
| `Elapsed(s)` | Время выполнения прогона в секундах                            |
| `Throughput` | Стратегий в секунду — основная метрика                         |
| `Speedup`    | Ускорение относительно baseline (worker=1)                     |
| `Errors`     | Инфраструктурные ошибки (nftables/nfqws2), не путать с FAILED  |
| `1*`         | Probe-прогон: 8 стратегий вместо полного набора для быстрого baseline. Нагрузка I/O-bound, throughput не зависит от количества стратегий |

**Алгоритм выбора оптимума:**
1. Отбросить точки с ошибками (errors > 0)
2. Найти максимальный throughput
3. Порог = 90% от максимума
4. Выбрать минимальное число воркеров, достигающее порога

**Флаги:**

| Флаг | Описание | По умолчанию |
|:-----|:---------|:-------------|
| `-s N` / `--strategies N` | Количество фейковых стратегий для прогона | 64 |
| `-M N` / `--max-workers N` | Верхняя граница поиска воркеров | CPU * 16 |
| `--raw` | Только таблица без рекомендации (для скриптов) | off |

**Примеры:**

```shell
# Быстрый тест на слабом железе
blockcheckw benchmark -s 16 -M 16

# Полный тест с расширенным диапазоном
blockcheckw benchmark -s 128 -M 256

# Для парсинга скриптом
blockcheckw benchmark --raw
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
