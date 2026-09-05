//! Prueba de extremo a extremo del camino de escritura (`Afc`, sentido
//! `down`) del adapter TCP real: un cliente de prueba se conecta al listener
//! de `lamula_ingest::tcp`, el test manda un `Afc` por `IngestSource::afc`, y
//! se comprueba que el cliente recibe exactamente los bytes que
//! `encode_afc_frame` produce. Hermana de `tcp_loopback.rs`, que prueba el
//! sentido inverso (`Ray`, up).

use lamula_contract::drx_dsp::Afc;
use lamula_ingest::encode_afc_frame;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const FULL_SCALE: i16 = i16::MAX;

#[tokio::test]
async fn afc_sent_after_connection_reaches_the_client() {
    let listener = lamula_ingest::tcp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let source = lamula_ingest::tcp::spawn(listener, FULL_SCALE, 16);

    let mut client = TcpStream::connect(local_addr).await.unwrap();
    // Deja que la tarea de lectura complete el `accept()` y registre la
    // mitad de escritura antes de mandar el `Afc` — sin esto la corrección
    // podría llegar antes de que `write_half` tenga `Some`, y el propio
    // contrato del módulo dice que en ese caso se descarta en silencio.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let afc = Afc {
        nco_phase_inc: 0xDEAD_BEEF_0000_1234,
        apply_at_seq: 7,
        pad0: 0,
    };
    source.afc.send(afc).await.unwrap();

    let expected = encode_afc_frame(&afc);
    let mut got = vec![0u8; expected.len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.read_exact(&mut got),
    )
    .await
    .expect("timeout esperando el Afc")
    .unwrap();
    assert_eq!(got, expected);

    source.task.abort();
}

#[tokio::test]
async fn afc_sent_before_any_connection_is_silently_dropped() {
    let listener = lamula_ingest::tcp::bind("127.0.0.1:0").await.unwrap();
    let source = lamula_ingest::tcp::spawn(listener, FULL_SCALE, 16);

    let afc = Afc {
        nco_phase_inc: 1,
        apply_at_seq: 0,
        pad0: 0,
    };
    // Sin cliente conectado: `send` sobre el canal en sí no falla (alguien
    // sigue drenándolo), pero no hay mitad de escritura a la que reenviarlo
    // — ver el doc-comment del módulo. No hay forma de observar el "no pasó
    // nada" salvo por ausencia; este test sólo confirma que `send` no
    // bloquea ni entra en pánico.
    tokio::time::timeout(std::time::Duration::from_millis(200), source.afc.send(afc))
        .await
        .expect("send no debería bloquear sin conexión")
        .unwrap();

    source.task.abort();
}
