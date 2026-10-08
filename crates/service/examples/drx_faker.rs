//! DRx de mentira: escucha, y cuando el DSP conecta le escupe rayos sintéticos por el cable
//! `DRx↔DSP` real, a cadencia de PRF.
//!
//! Para qué existe: hasta ahora el pipeline sólo se había alimentado con
//! `lamula_ingest::simulator`, que es un adapter **en memoria** — no toca
//! socket, ni framing, ni contrapresión. Este ejemplo empuja las mismas
//! tramas que genera `lamula_simulator::pack_rays` por TCP de verdad, contra
//! `lamula_ingest::tcp`, que es el código que un día recibirá a la ZedBoard.
//! Es la pieza que falta para levantar el banco RCP-real ↔ DSP-real sin
//! hardware (fase C0 del plan de integración; ver el pendiente P-13 del
//! proyecto DRx).
//!
//! Lo que **no** es: no es un modelo del DRx. No hay DDC, ni range gating, ni
//! timing real, ni jitter de hardware, ni lectores SSI — el azimut avanza un
//! paso fijo por radial porque hace falta que avance, no porque se parezca a
//! una antena. Sirve para ejercitar transporte, framing, ensamblado de
//! radiales y el camino aguas abajo hasta el RCP. Para medir cualquier
//! propiedad del hardware, no vale: eso es la fase B, con la placa.
//!
//! Uso:
//!
//! ```sh
//! cargo run --example drx_faker -- 0.0.0.0:9470 [radiales]
//! ```
//!
//! Sin argumentos toma `LAMULA_DSP_DRX_ADDR` del entorno y emite sin parar.
//! Escucha, como la placa (D-16 del proyecto DRx): el DSP conecta, y si el DSP
//! se cae el faker vuelve a esperar al siguiente sin reiniciarse.

use std::env;
use std::time::Duration;

use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

/// Pulsos por radial. Mismo valor que usa el banco vertical del workspace.
const M: usize = 64;
/// Celdas de rango por rayo.
const N_GATES: usize = 128;
const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const FULL_SCALE: i16 = i16::MAX;
/// Cuentas de encoder por vuelta. 16 bits, no los 4096 de los tests: con
/// 4096 cuentas un radial de 64 pulsos no puede avanzar ni una cuenta por
/// pulso sin cubrir 5 grados, y hace falta que avance (ver
/// `azimuth_step_raw`). Quien lance esto tiene que poner el mismo valor en
/// `LAMULA_DSP_SSI_COUNTS_PER_TURN` del DSP, o los grados no cuadrarán.
const COUNTS_PER_TURN: u32 = 65_536;
/// Avance de azimut por pulso. Con M=64 pulsos sale un radial de 128 cuentas,
/// unos 0,70 grados — del orden de un radial de verdad.
const AZ_STEP_PER_PULSE: u32 = 2;
/// Avance de azimut por radial, en cuentas.
const AZ_STEP: u32 = AZ_STEP_PER_PULSE * M as u32;

#[tokio::main]
async fn main() {
    let mut args = env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| {
        env::var("LAMULA_DSP_DRX_ADDR").unwrap_or_else(|_| {
            eprintln!("uso: drx_faker <host:puerto> [radiales]  (o LAMULA_DSP_DRX_ADDR)");
            std::process::exit(2);
        })
    });
    let max_radials: Option<u64> = args.next().map(|s| {
        s.parse()
            .unwrap_or_else(|_| panic!("el número de radiales no es un entero: {s}"))
    });

    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("drx_faker: no se pudo escuchar en {addr}: {e}"));
    println!("drx_faker: escuchando en {addr}");
    let mut socket = accept_dsp(&listener).await;

    let mut rng = StdRng::seed_from_u64(2026);
    let mut radial: u64 = 0;
    let mut seq: u32 = 0;
    let mut timestamp_ns: u64 = 0;

    loop {
        if let Some(max) = max_radials {
            if radial >= max {
                println!("drx_faker: {radial} radiales emitidos, fin");
                return;
            }
        }

        // Velocidad media que varía con el azimut: así el RCP no recibe
        // radiales idénticos y se nota de un vistazo si el flujo avanza.
        let mean_v = 10.0 * ((radial as f64) * 0.05).sin();
        let cells: Vec<_> = (0..N_GATES)
            .map(|bin| {
                let params = CellParams {
                    // Potencia decreciente con el rango, sin pretensión de
                    // ser una ecuación radar: sólo para que los momentos no
                    // salgan planos.
                    power_s: 0.05 / (1.0 + bin as f64 * 0.05),
                    mean_v,
                    sigma_v: 1.0,
                    wavelength_m: WAVELENGTH_M,
                    prt_s: PRT_S,
                    m: M,
                    noise_floor: 1.0e-4,
                };
                generate_cell(&params, &mut rng)
            })
            .collect();

        let fields = RayHeaderFields {
            seq_start: seq,
            timestamp_ns_start: timestamp_ns,
            timestamp_step_ns: (PRT_S * 1.0e9) as u64,
            trigger_count_start: seq,
            azimuth_raw: ((radial as u32).wrapping_mul(AZ_STEP)) % COUNTS_PER_TURN,
            // La antena no se para durante el radial: el azimut avanza pulso
            // a pulso, para que el radial tenga un sector real y no un ancho
            // de cero (que no es archivable como Level-II).
            azimuth_step_raw: AZ_STEP_PER_PULSE,
            elevation_raw: 0,
            prf_div: 4,
            pulse_width_idx: 0,
            pulse_mode: 0,
            cell_mode: 0,
            channel_mask: 0b0001,
            ray_flags: 0,
        };

        for frame in pack_rays(&fields, &[cells], FULL_SCALE) {
            if let Err(e) = socket.write_all(&frame).await {
                eprintln!("drx_faker: enlace caído ({e}); esperando al DSP");
                socket = accept_dsp(&listener).await;
                break;
            }
        }

        seq = seq.wrapping_add(M as u32);
        timestamp_ns += (PRT_S * 1.0e9) as u64 * M as u64;
        radial += 1;
        // Cadencia: un radial dura M·PRT. Sin esto el DSP recibe el volumen
        // entero de golpe y no se ejercita nada parecido a un flujo real.
        tokio::time::sleep(Duration::from_secs_f64(PRT_S * M as f64)).await;
    }
}

async fn accept_dsp(listener: &TcpListener) -> tokio::net::TcpStream {
    loop {
        match listener.accept().await {
            Ok((socket, peer)) => {
                println!("drx_faker: DSP conectado desde {peer}");
                return socket;
            }
            Err(e) => {
                eprintln!("drx_faker: accept falló ({e}); reintento en 1 s");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
