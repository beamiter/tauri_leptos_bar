use chrono::{Datelike, Local, Timelike};
use gloo_console::error;
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::{MouseEvent, WheelEvent};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen, catch)]
    async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Default)]
struct TagStatus {
    is_selected: bool,
    is_urg: bool,
    is_filled: bool,
    is_occ: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
struct MonitorInfoSnapshot {
    monitor_num: i32,
    monitor_width: i32,
    monitor_height: i32,
    monitor_x: i32,
    monitor_y: i32,
    tag_status_vec: Vec<TagStatus>,
    client_name: String,
    ltsymbol: String,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
struct SystemSnapshot {
    cpu_average: f32,
    memory_used: u64,
    memory_total: u64,
    memory_usage_percent: f32,
    battery_percent: f32,
    is_charging: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
struct AudioSnapshot {
    volume: i32,
    is_muted: bool,
    device_name: String,
    has_device: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
struct BrightnessSnapshot {
    percent: Option<u8>,
}

#[derive(Deserialize)]
struct EventPayload<T> {
    payload: T,
}

const TAG_ICONS: [&str; 9] = [
    "\u{F0A1E}",
    "\u{F0239}",
    "\u{F0A1B}",
    "\u{F0B79}",
    "\u{F024B}",
    "\u{F0388}",
    "\u{F0567}",
    "\u{F01F0}",
    "\u{F0297}",
];

const ICON_CPU: &str = "\u{F4BC}";
const ICON_MEM: &str = "\u{F035B}";
const ICON_BAT_FULL: &str = "\u{F0079}";
const ICON_BAT_CHG: &str = "\u{F0084}";
const ICON_VOL_HIGH: &str = "\u{F057E}";
const ICON_VOL_MID: &str = "\u{F0580}";
const ICON_VOL_LOW: &str = "\u{F057F}";
const ICON_VOL_MUTE: &str = "\u{F075F}";
const ICON_BRIGHT: &str = "\u{F00DE}";
const ICON_SHOT: &str = "\u{F0104}";
const ICON_TIME: &str = "\u{F0954}";
const ICON_MON: &str = "\u{F0379}";

fn button_class(t: &TagStatus) -> &'static str {
    if t.is_filled {
        "emoji-button state-filtered"
    } else if t.is_selected {
        "emoji-button state-selected"
    } else if t.is_urg {
        "emoji-button state-urgent"
    } else if t.is_occ {
        "emoji-button state-occupied"
    } else {
        "emoji-button state-default"
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0B".to_string();
    }
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let i = ((bytes as f64).ln() / 1024f64.ln()).floor() as usize;
    let i = i.min(U.len() - 1);
    let s = bytes as f64 / 1024f64.powi(i as i32);
    if i == 0 {
        format!("{:.0}{}", s, U[i])
    } else {
        format!("{:.1}{}", s, U[i])
    }
}

fn parse_lt_symbol(lts: &str) -> (String, Option<f32>) {
    if lts.is_empty() {
        return ("[]=".to_string(), None);
    }
    let symbol = lts
        .split_whitespace()
        .next()
        .unwrap_or("[]=")
        .to_string();
    let scale = lts.find("s:").and_then(|i| {
        let rest = &lts[i + 2..];
        let s: String = rest
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        s.parse::<f32>().ok()
    });
    (symbol, scale)
}

fn monitor_icon(n: i32) -> String {
    if n == 0 {
        "\u{F02DA}".to_string()
    } else if n == 1 {
        "\u{F02DB}".to_string()
    } else {
        format!("M{}", n)
    }
}

fn sev(p: f32) -> &'static str {
    if p <= 30.0 {
        "usage-good"
    } else if p <= 60.0 {
        "usage-warn"
    } else if p <= 80.0 {
        "usage-caution"
    } else {
        "usage-danger"
    }
}

fn volume_icon(a: Option<&AudioSnapshot>) -> &'static str {
    match a {
        None => ICON_VOL_MUTE,
        Some(s) => {
            if !s.has_device || s.is_muted || s.volume <= 0 {
                ICON_VOL_MUTE
            } else if s.volume < 34 {
                ICON_VOL_LOW
            } else if s.volume < 67 {
                ICON_VOL_MID
            } else {
                ICON_VOL_HIGH
            }
        }
    }
}

fn invoke_async(cmd: &'static str, args: JsValue) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = tauri_invoke(cmd, args).await {
            error!(format!("invoke {} failed: {:?}", cmd, e));
        }
    });
}

#[derive(Serialize)]
struct TagCmdArgs {
    #[serde(rename = "tagIndex")]
    tag_index: usize,
    #[serde(rename = "isView")]
    is_view: bool,
    #[serde(rename = "monitorId")]
    monitor_id: i32,
}

#[derive(Serialize)]
struct LayoutCmdArgs {
    #[serde(rename = "layoutIndex")]
    layout_index: u32,
    #[serde(rename = "monitorId")]
    monitor_id: i32,
}

#[derive(Serialize)]
struct DeltaArgs {
    delta: i32,
}

#[component]
fn App() -> impl IntoView {
    let (monitor, set_monitor) = signal(None::<MonitorInfoSnapshot>);
    let (system, set_system) = signal(None::<SystemSnapshot>);
    let (audio, set_audio) = signal(None::<AudioSnapshot>);
    let (brightness, set_brightness) = signal(None::<BrightnessSnapshot>);
    let (pressed, set_pressed) = signal(None::<usize>);
    let (layout_open, set_layout_open) = signal(false);
    let (show_seconds, set_show_seconds) = signal(true);
    let (now, set_now) = signal(Local::now());
    let (is_taking, set_is_taking) = signal(false);

    // listen monitor-update
    Effect::new(move |_| {
        let cb = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            if let Ok(p) = serde_wasm_bindgen::from_value::<EventPayload<MonitorInfoSnapshot>>(evt) {
                set_monitor.set(Some(p.payload));
            }
        });
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri_listen("monitor-update", &cb).await {
                error!(format!("listen failed: {:?}", e));
            }
            cb.forget();
        });
    });

    // listen system-update
    Effect::new(move |_| {
        let cb = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            if let Ok(p) = serde_wasm_bindgen::from_value::<EventPayload<SystemSnapshot>>(evt) {
                set_system.set(Some(p.payload));
            }
        });
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri_listen("system-update", &cb).await {
                error!(format!("listen failed: {:?}", e));
            }
            cb.forget();
        });
    });

    // listen audio-update
    Effect::new(move |_| {
        let cb = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            if let Ok(p) = serde_wasm_bindgen::from_value::<EventPayload<AudioSnapshot>>(evt) {
                set_audio.set(Some(p.payload));
            }
        });
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri_listen("audio-update", &cb).await {
                error!(format!("listen failed: {:?}", e));
            }
            cb.forget();
        });
    });

    // listen brightness-update
    Effect::new(move |_| {
        let cb = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
            if let Ok(p) = serde_wasm_bindgen::from_value::<EventPayload<BrightnessSnapshot>>(evt) {
                set_brightness.set(Some(p.payload));
            }
        });
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri_listen("brightness-update", &cb).await {
                error!(format!("listen failed: {:?}", e));
            }
            cb.forget();
        });
    });

    // tick clock — re-arm interval whenever show_seconds changes
    Effect::new(move |prev: Option<Interval>| {
        drop(prev);
        let secs = show_seconds.get();
        let interval_ms = if secs { 1000 } else { 60000 };
        Interval::new(interval_ms, move || set_now.set(Local::now()))
    });

    let formatted_time = Memo::new(move |_| {
        let d = now.get();
        let pad = |n: u32| format!("{:02}", n);
        let ts = if show_seconds.get() {
            format!("{}:{}:{}", pad(d.hour()), pad(d.minute()), pad(d.second()))
        } else {
            format!("{}:{}", pad(d.hour()), pad(d.minute()))
        };
        format!("{}-{}-{} {}", d.year(), pad(d.month()), pad(d.day()), ts)
    });

    let lt = Memo::new(move |_| match monitor.get() {
        Some(m) => parse_lt_symbol(&m.ltsymbol),
        None => ("[]=".to_string(), None),
    });

    let take_screenshot = move |_| {
        if is_taking.get() {
            return;
        }
        set_is_taking.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri_invoke("take_screenshot", JsValue::NULL).await {
                error!(format!("screenshot failed: {:?}", e));
            }
            gloo_timers::future::TimeoutFuture::new(500).await;
            set_is_taking.set(false);
        });
    };

    view! {
        <Show
            when=move || monitor.get().is_some()
            fallback=|| view! { <div class="button-row">"Loading..."</div> }
        >
            {move || {
                let m = monitor.get().unwrap();
                let monitor_num = m.monitor_num;
                let tags = m.tag_status_vec.clone();

                view! {
                    <div class="button-row">
                        <div class="buttons-container">
                            {
                                TAG_ICONS.iter().enumerate().map(|(i, icon)| {
                                    let tag = tags.get(i).cloned().unwrap_or_default();
                                    let base = button_class(&tag);
                                    let cls = move || {
                                        if pressed.get() == Some(i) {
                                            format!("{} pressed", base)
                                        } else {
                                            base.to_string()
                                        }
                                    };
                                    view! {
                                        <button
                                            class=cls
                                            on:mousedown=move |_| set_pressed.set(Some(i))
                                            on:mouseup=move |_| {
                                                set_pressed.set(None);
                                                let args = serde_wasm_bindgen::to_value(&TagCmdArgs {
                                                    tag_index: i,
                                                    is_view: true,
                                                    monitor_id: monitor_num,
                                                }).unwrap_or(JsValue::NULL);
                                                invoke_async("send_tag_command", args);
                                            }
                                            on:mouseleave=move |_| set_pressed.set(None)
                                            title=format!("Tag {}", i + 1)
                                        >
                                            <span class="nf-icon">{*icon}</span>
                                        </button>
                                    }
                                }).collect_view()
                            }

                            <div class="layout-controls">
                                <div
                                    class=move || {
                                        if layout_open.get() {
                                            "pill layout-toggle open"
                                        } else {
                                            "pill layout-toggle closed"
                                        }
                                    }
                                    on:click=move |_| set_layout_open.update(|v| *v = !*v)
                                    title="切换布局"
                                >
                                    {move || lt.get().0}
                                </div>
                                <Show when=move || layout_open.get() fallback=|| ()>
                                    <div class="layout-selector">
                                        {
                                            [("[]=", 0u32), ("><>", 1u32), ("[M]", 2u32)]
                                                .into_iter()
                                                .map(|(label, idx)| {
                                                    let label_owned = label.to_string();
                                                    let cls = move || {
                                                        if lt.get().0 == label_owned {
                                                            "pill layout-option current"
                                                        } else {
                                                            "pill layout-option"
                                                        }
                                                    };
                                                    view! {
                                                        <div
                                                            class=cls
                                                            on:click=move |_| {
                                                                set_layout_open.set(false);
                                                                let args = serde_wasm_bindgen::to_value(&LayoutCmdArgs {
                                                                    layout_index: idx,
                                                                    monitor_id: monitor_num,
                                                                }).unwrap_or(JsValue::NULL);
                                                                invoke_async("send_layout_command", args);
                                                            }
                                                        >
                                                            {label.to_string()}
                                                        </div>
                                                    }
                                                })
                                                .collect_view()
                                        }
                                    </div>
                                </Show>
                            </div>
                        </div>

                        <div class="spacer"></div>

                        <div class="right-info-container">
                            <div class="system-info-container">
                                {move || match system.get() {
                                    Some(s) => {
                                        let cpu_cls = format!("pill usage-pill {}", sev(s.cpu_average));
                                        let mem_cls = format!("pill usage-pill {}", sev(s.memory_usage_percent));
                                        let batt_cls = format!("pill usage-pill {}",
                                            if s.battery_percent > 50.0 { "usage-good" }
                                            else if s.battery_percent > 20.0 { "usage-warn" }
                                            else { "usage-danger" });
                                        let batt_icon = if s.is_charging { ICON_BAT_CHG } else { ICON_BAT_FULL };
                                        let mem_title = format!("内存使用: {} / {}", format_bytes(s.memory_used), format_bytes(s.memory_total));
                                        let batt_title = if s.is_charging {
                                            format!("电池充电中: {:.1}%", s.battery_percent)
                                        } else {
                                            format!("电池电量: {:.1}%", s.battery_percent)
                                        };
                                        view! {
                                            <>
                                                <div class=cpu_cls title="CPU 平均使用率">
                                                    <span class="nf-icon">{ICON_CPU}</span>
                                                    {format!(" {:.0}%", s.cpu_average)}
                                                </div>
                                                <div class=mem_cls title=mem_title>
                                                    <span class="nf-icon">{ICON_MEM}</span>
                                                    {format!(" {:.0}%", s.memory_usage_percent)}
                                                </div>
                                                <div class=batt_cls title=batt_title>
                                                    <span class="nf-icon">{batt_icon}</span>
                                                    {format!(" {:.0}%", s.battery_percent)}
                                                </div>
                                            </>
                                        }.into_any()
                                    }
                                    None => view! {
                                        <>
                                            <div class="pill usage-pill usage-warn">
                                                <span class="nf-icon">{ICON_CPU}</span>" --%"
                                            </div>
                                            <div class="pill usage-pill usage-warn">
                                                <span class="nf-icon">{ICON_MEM}</span>" --%"
                                            </div>
                                            <div class="pill usage-pill usage-warn">
                                                <span class="nf-icon">{ICON_BAT_FULL}</span>" --%"
                                            </div>
                                        </>
                                    }.into_any()
                                }}
                            </div>

                            <div
                                class="pill brightness-pill"
                                on:click=move |_| {
                                    let args = serde_wasm_bindgen::to_value(&DeltaArgs { delta: 5 }).unwrap_or(JsValue::NULL);
                                    invoke_async("adjust_brightness", args);
                                }
                                on:wheel=move |e: WheelEvent| {
                                    e.prevent_default();
                                    let delta = if e.delta_y() < 0.0 { 5 } else { -5 };
                                    let args = serde_wasm_bindgen::to_value(&DeltaArgs { delta }).unwrap_or(JsValue::NULL);
                                    invoke_async("adjust_brightness", args);
                                }
                                on:contextmenu=move |e: MouseEvent| {
                                    e.prevent_default();
                                    let args = serde_wasm_bindgen::to_value(&DeltaArgs { delta: -5 }).unwrap_or(JsValue::NULL);
                                    invoke_async("adjust_brightness", args);
                                }
                                title="左键加亮 / 右键减暗 / 滚轮调节"
                            >
                                <span class="nf-icon">{ICON_BRIGHT}</span>
                                {move || match brightness.get().and_then(|b| b.percent) {
                                    Some(p) => format!(" {}%", p),
                                    None => " --".to_string(),
                                }}
                            </div>

                            <div
                                class=move || {
                                    let muted = match audio.get() {
                                        None => true,
                                        Some(s) => s.is_muted || !s.has_device,
                                    };
                                    if muted { "pill volume-pill muted" } else { "pill volume-pill" }
                                }
                                on:click=move |_| invoke_async("toggle_mute", JsValue::NULL)
                                on:wheel=move |e: WheelEvent| {
                                    e.prevent_default();
                                    let delta = if e.delta_y() < 0.0 { 5 } else { -5 };
                                    let args = serde_wasm_bindgen::to_value(&DeltaArgs { delta }).unwrap_or(JsValue::NULL);
                                    invoke_async("adjust_volume", args);
                                }
                                title="左键静音 / 滚轮调节"
                            >
                                <span class="nf-icon">{move || volume_icon(audio.get().as_ref())}</span>
                                {move || match audio.get() {
                                    Some(s) if s.has_device => format!(" {}%", s.volume),
                                    _ => " --".to_string(),
                                }}
                            </div>

                            <div
                                class=move || {
                                    if is_taking.get() {
                                        "pill screenshot-pill taking"
                                    } else {
                                        "pill screenshot-pill"
                                    }
                                }
                                on:click=take_screenshot
                                title="截图 (Flameshot)"
                            >
                                <span class="nf-icon">{ICON_SHOT}</span>
                            </div>

                            <div
                                class="pill time-pill"
                                on:click=move |_| set_show_seconds.update(|v| *v = !*v)
                                title="点击切换秒显示"
                            >
                                <span class="nf-icon">{ICON_TIME}</span>
                                {move || format!(" {}", formatted_time.get())}
                            </div>

                            <div class="pill monitor-pill" title="显示器">
                                <span class="nf-icon">{ICON_MON}</span>
                                {format!(" {}", monitor_icon(monitor_num))}
                            </div>

                            <div class="pill scale-pill" title="Scale Factor">
                                {move || match lt.get().1 {
                                    Some(s) => format!("s: {:.2}", s),
                                    None => "s: --".to_string(),
                                }}
                            </div>
                        </div>
                    </div>
                }
            }}
        </Show>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to(
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("root")
            .unwrap()
            .unchecked_into(),
        App,
    )
    .forget();
}
