# Blockcheck Wrapper

**blockcheckw** — параллельный сканер стратегий обхода DPI. Переписан с `bash` на `Rust`.
Оригинальный `blockcheck2.sh` проверяет стратегии последовательно — одну за другой.
`blockcheckw` запускает их параллельно, изолируя воркеры через выделенные диапазоны портов.

## Как это работает

Каждый воркер получает изолированный слот:
- Уникальный диапазон source-портов (sport) для curl `--local-port`
- Персональное nftables-правило, матчащее только его sport-диапазон
- Свой экземпляр nfqws2 на выделенной NFQUEUE

Это позволяет запускать десятки стратегий одновременно без конфликтов.

### Pipeline команды scan

```
DNS resolve → Baseline → Generate strategies → Run parallel → Summary
```

1. **DNS resolve** — резолвит домен через `getent ahostsv4`, fallback на `nslookup`
2. **Baseline** — проверяет каждый протокол без bypass (curl без `--local-port`), определяет заблокированные
3. **Generate** — генерирует все стратегии для заблокированных протоколов (2449 HTTP / 9828 TLS1.3 / 19644 TLS1.2)
4. **Run parallel** — прогоняет стратегии параллельно через worker pool
5. **Summary** — выводит найденные рабочие стратегии

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

### Unit-тесты
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

## Запуск

### Scan — поиск рабочих стратегий

```shell
blockcheckw scan
```

По умолчанию сканирует `rutracker.org` по всем трём протоколам (HTTP, TLS1.2, TLS1.3).
Количество воркеров задаётся глобальным флагом `-w`.

```
=== DNS resolve ===
resolved rutracker.org -> 172.67.182.217

=== Baseline (without bypass) ===
  HTTP: BLOCKED (UNAVAILABLE code=28)
  HTTPS/TLS1.2: BLOCKED (UNAVAILABLE code=28)
  HTTPS/TLS1.3: available without bypass

Blocked protocols: HTTP, HTTPS/TLS1.2

=== Scanning HTTP ===
  generated 2449 strategies, workers=64
  ...
  completed: 2449 | success: 12 | failed: 2437 | errors: 0 | 114.3s (21.4 strat/sec)

=== Summary for rutracker.org ===
  HTTPS/TLS1.3: working without bypass
  HTTP: 12 working strategies found
    nfqws2 --payload=http_req --lua-desync=fake:blob=fake_default_http:ip_ttl=4:repeats=1
    ...
  HTTPS/TLS1.2: no working strategies found
```

**Флаги:**

| Флаг | Описание | По умолчанию |
|:-----|:---------|:-------------|
| `-d` / `--domain` | Домен для проверки | `rutracker.org` |
| `-p` / `--protocols` | Протоколы через запятую: `http`, `tls12`, `tls13` | `http,tls12,tls13` |

**Примеры:**

```shell
# Только HTTP с 64 воркерами
blockcheckw -w 64 scan -p http

# Конкретный домен, только TLS
blockcheckw -w 32 scan -d example.com -p tls12,tls13

# Все протоколы (по умолчанию)
blockcheckw -w 64 scan
```

## Производительность

Результаты нагрузочного теста — 1000 стратегий, NanoPi R3S:

| Workers | Время | Throughput | Ускорение |
|--------:|------:|-----------:|----------:|
| 1       | ~17m  |  ~1.0/sec  |     1.0x  |
| **64**  |**46.7s**|**21.4/sec**| **~21x** |

Масштабирование почти линейное до 64 воркеров. 0 ошибок.

### Тестовый стенд

- **Роутер**: FriendlyElec NanoPi R3S
- **CPU**: 4x ARM Cortex-A53
- **RAM**: 2 GB
- **OS**: OpenWrt 25.12, kernel 6.12
- **Бинарник**: статически слинкованный `aarch64-unknown-linux-musl`, 2.6 MB

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
