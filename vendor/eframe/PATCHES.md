# Záplaty oproti eframe 0.33.3 (crates.io)

Upstream: MIT OR Apache-2.0 (viz Cargo.toml). Tohle je kopie 0.33.3 + čtyři malé změny.
Až půjde eframe aktualizovat na verzi, která opravu už obsahuje (egui#8326, eframe ≥ 0.36),
smaž `vendor/eframe` a sekci `[patch.crates-io]` v kořenovém Cargo.toml.

## Proč
Na Windows skryté okno nikdy nedostane `RedrawRequested` (WM_PAINT se doručuje jen viditelným oknům).
`check_redraw_requests` ale při každém naplánovaném překreslení nastavil `ControlFlow::Poll` a zavolal
`request_redraw()`. U skrytého okna nikdo neodpověděl, nic nevrátilo smyčku do `Wait` -> winit smyčka
se točila na 100 % jednoho vlákna, dokud okno někdo neukázal.
Naplánované překreslení přitom skryté okno dostane vždycky: při schování přijde `Focused(false)`
a egui si při každém průchodu se vstupní událostí samo vyžádá další překreslení.

## Změny
- `src/native/run.rs`
  - `check_redraw_requests`: skryté / minimalizované okno se vykreslí přímo (ne přes WM_PAINT),
    nejvýš jednou za 100 ms; viditelné okno dostane `request_redraw()` bez `Poll`;
    na konci se vždy nastaví `WaitUntil(..)` nebo `Wait` (starý `Poll` se tak nemůže „zapomenout“).
  - `user_event`: požadavky na překreslení skrytého okna se zpozdí o 100 ms.
- `src/native/winit_integration.rs`: pomocná `is_invisible_or_minimized`.
- `src/native/glow_integration.rs`: `Instant::now() + info.delay` -> `checked_add` (s `Duration::MAX` to dřív paniklo).
- `src/native/epi_integration.rs`: po prvním snímku se okno ukáže jen když aplikace nežádala
  `ViewportBuilder::with_visible(false)` (start rovnou do tray); dřív se ukázalo vždycky.
