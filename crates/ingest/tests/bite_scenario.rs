//! Escenario guionado de BITE de punta a punta (`docs/dsp-plan.md`
//! §"fault injection ... for BITE testing"): una sola ráfaga con tres fallas
//! inyectadas a la vez -- una trama corrupta (`BadMagic`), un pulso perdido
//! en el enlace y un glitch de encoder en otro -- corriendo por el mismo
//! camino real que `vertical_slice.rs`
//! (`lamula_ingest::simulator` → `RadialAssembler` → `pulse_pair_moments`).
//! Cierra el hueco que `docs/algorithms/roadmap.md` señalaba en la entrada de
//! inyección de fallos de `crates/simulator`: los generadores de fallos ya
//! existían, pero nada los consumía todavía desde un guion de punta a punta.
//!
//! Antes de que `crates/ingest::simulator::spawn` contara y descartara una
//! trama rechazada por `decode_ray_frame` en vez de propagar el error (ver el
//! doc-comment de `tcp::spawn`, mismo cambio en los tres adapters), la propia
//! trama `BadMagic` de este escenario habría tumbado la tarea de ingesta
//! entera y ninguna trama posterior habría llegado nunca -- exactamente el
//! caso que este test existe para cubrir.
//!
//! Criterio de aceptación de este test, deliberadamente modesto y a
//! propósito NO numérico de exactitud (mismo criterio que
//! `crates/service/tests/soak.rs`: "ray_seq monótono, ningún momento sale
//! infinito", no "el valor es exacto"): que el radial se complete sin pánico
//! con exactamente las muestras que sobrevivieron, que los dos contadores
//! (`malformed_frames`, `dropped_pulses`) reflejen las tres fallas
//! correctamente, y que el estimador de velocidad sobre la serie resultante
//! -- con dos muestras "agujereadas" en medio, no rellenadas -- siga dando un
//! número finito y dentro del rango de Nyquist, no basura. Una curva de
//! exactitud real de momentos-bajo-falla-inyectada (sesgo vs. tipo/posición
//! de falla) es trabajo de oráculo aparte, no inventado aquí sin poder
//! correr `cargo test` para contrastarlo en este entorno.

use lamula_ingest::RadialAssembler;
use lamula_moments::{pulse_pair_moments, PulsePairFlag};
use lamula_simulator::{
    corrupt_frame, drop_rays, generate_cell, inject_encoder_glitch, pack_rays, CellParams,
    FrameFault, RayHeaderFields,
};
use rand::rngs::StdRng;
use rand::SeedableRng;

const FULL_SCALE: i16 = i16::MAX;

#[tokio::test]
async fn scripted_bite_scenario_survives_corrupt_dropped_and_glitched_pulses() {
    // Pulsos que deben sobrevivir para completar el radial (n_pulses del
    // `RadialAssembler`). Se generan M+2: uno se corrompe (índice 2, se
    // rechaza antes de llegar al ensamblador), otro se descarta (índice 10,
    // nunca sale del "enlace"), así que llegan exactamente M al ensamblador.
    const M: usize = 32;
    let params = CellParams {
        power_s: 0.01,
        mean_v: 8.0,
        sigma_v: 1.0,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: M + 2,
        noise_floor: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(20_260_905);
    let cell = generate_cell(&params, &mut rng);

    let fields = RayHeaderFields {
        seq_start: 0,
        timestamp_ns_start: 0,
        timestamp_step_ns: (params.prt_s * 1.0e9) as u64,
        trigger_count_start: 0,
        azimuth_raw: 100,
        elevation_raw: 5,
        prf_div: 4,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let mut wire_frames = pack_rays(&fields, &[vec![cell]], FULL_SCALE);
    assert_eq!(wire_frames.len(), M + 2);

    // Trama 2: `BadMagic` -- el adapter la rechaza, la cuenta en
    // `malformed_frames`, y no llega nunca al `RadialAssembler`.
    wire_frames[2] = corrupt_frame(&wire_frames[2], FrameFault::BadMagic);
    // Trama 0 (sobrevive): glitch de encoder. Sólo debe cambiar
    // azimuth_raw/elevation_raw -- no debe impedir que decodifique ni que el
    // radial se complete.
    inject_encoder_glitch(&mut wire_frames[0], 999, 999);
    // Trama 10: pulso perdido en el enlace -- a diferencia de la corrupta, ni
    // siquiera se manda.
    let wire_frames = drop_rays(wire_frames, &[10]);
    assert_eq!(wire_frames.len(), M + 1, "quedan M+2 menos la descartada");

    let mut source = lamula_ingest::simulator::spawn(wire_frames, FULL_SCALE, M + 2);

    let mut radial = None;
    let mut assembler = RadialAssembler::new(M as u16);
    while radial.is_none() {
        let frame = source
            .frames
            .recv()
            .await
            .expect("el radial debería completarse con las tramas que sobrevivieron");
        radial = assembler.feed(frame).unwrap();
    }
    let radial = radial.unwrap();
    // Sin más tramas por mandar (la corrupta y la descartada ya se fueron):
    // la tarea debe terminar sola con `Ok(())`, no quedarse colgada ni
    // haber terminado por error -- justo lo que antes de este cambio no
    // pasaba ante la trama `BadMagic`.
    source.task.await.unwrap().unwrap();

    assert_eq!(
        source
            .malformed_frames
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "la trama BadMagic debe contarse, no colar silenciosamente ni matar la ingesta"
    );
    assert_eq!(
        radial.dropped_pulses, 2,
        "un hueco de seq por la trama corrupta (nunca llega al ensamblador) y otro por la \
         descartada -- ambas faltas se ven igual desde `RadialAssembler`, que no distingue \
         por qué falta un seq"
    );
    assert_eq!(
        radial.azimuth_raw, 999,
        "el glitch de encoder llega tal cual hasta el radial ensamblado -- filtrarlo/marcarlo \
         es trabajo de una capa de BITE que no existe todavía en este workspace"
    );
    assert_eq!(radial.channels[0][0].len(), M);

    let estimate = pulse_pair_moments(&radial.channels[0][0], params.wavelength_m, params.prt_s);
    assert_ne!(
        estimate.flag,
        PulsePairFlag::Censored,
        "con power_s=0.01 y sin ruido la celda no debería censurarse por SNR; \
         `Saturated` (ancho recortado a cero) sí sería un resultado válido, sólo \
         `Censored` (S<=0) señalaría que la velocidad no tiene sentido físico aquí"
    );
    let v_nyquist = params.wavelength_m / (4.0 * params.prt_s);
    assert!(
        estimate.velocity_mps.is_finite() && estimate.velocity_mps.abs() <= v_nyquist,
        "velocidad {} fuera del rango físico posible [-{v_nyquist}, {v_nyquist}] -- \
         degradación silenciosa, no sólo imprecisión",
        estimate.velocity_mps
    );
}
