//! Prueba de extremo a extremo del adapter TCP real: un cliente de prueba
//! hace de RCP. Manda una trama `down` construida a mano y comprueba que
//! llega decodificada por `link.down`; manda un `UpMessage` por `link.up` y
//! comprueba los bytes crudos que salen por el socket.

use lamula_contract::dsp_rcp::{self, Control, MsgType, Status, CONTROL_SIZE, HEADER_SIZE, MAGIC};
use lamula_rcp_link::wire::{DownMessage, UpMessage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn header_bytes(msg_type: MsgType, payload_len: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(dsp_rcp::VERSION_MAJOR);
    buf.push(dsp_rcp::VERSION_MINOR);
    buf.push(msg_type as u8);
    buf.push(0);
    buf.extend_from_slice(&payload_len.to_le_bytes());
    buf
}

#[tokio::test]
async fn control_frame_from_rcp_arrives_decoded() {
    let listener = lamula_rcp_link::tcp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let mut link = lamula_rcp_link::tcp::spawn(listener, 16, 16);

    let mut client = TcpStream::connect(local_addr).await.unwrap();
    let control = Control {
        seq: 3,
        command: dsp_rcp::command::REQUEST_STATUS,
        pad0: 0,
        pad1: 0,
    };
    let mut buf = header_bytes(MsgType::Control, CONTROL_SIZE as u32);
    buf.extend_from_slice(&control.seq.to_le_bytes());
    buf.push(control.command);
    buf.push(control.pad0);
    buf.extend_from_slice(&control.pad1.to_le_bytes());
    client.write_all(&buf).await.unwrap();

    let got = link.down.recv().await.expect("falta el mensaje down");
    assert_eq!(got, DownMessage::Control(control));

    drop(client);
    drop(link.up); // soltar `up` es la señal de apagado intencional del enlace
    link.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn status_sent_up_arrives_on_the_wire() {
    let listener = lamula_rcp_link::tcp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let link = lamula_rcp_link::tcp::spawn(listener, 16, 16);

    let mut client = TcpStream::connect(local_addr).await.unwrap();

    let status = Status {
        uptime_s: 7,
        phase: dsp_rcp::phase::RUNNING,
        severity: dsp_rcp::severity::INFO,
        last_error: 0,
        n_rx_channels: 1,
        capability_flags: 0,
        bite_flags: 0,
        config_seq: 1,
        rays_in: 10,
        rays_out: 10,
        rays_dropped: 0,
        queue_depth: 0,
        bins_ok: 100,
        bins_total: 100,
        trigger_period_cmd_ns: 1000,
        trigger_period_meas_ns: 1000,
        pad0: 0,
        noise_floor_dbm_0: -110.0,
        noise_floor_dbm_1: 0.0,
        noise_floor_dbm_2: 0.0,
        noise_floor_dbm_3: 0.0,
        dc_offset_i_0: 0.0,
        dc_offset_i_1: 0.0,
        dc_offset_i_2: 0.0,
        dc_offset_i_3: 0.0,
        dc_offset_q_0: 0.0,
        dc_offset_q_1: 0.0,
        dc_offset_q_2: 0.0,
        dc_offset_q_3: 0.0,
    };
    link.up.send(UpMessage::Status(status)).await.unwrap();

    let expected_len = HEADER_SIZE + dsp_rcp::STATUS_SIZE;
    let mut got = vec![0u8; expected_len];
    client.read_exact(&mut got).await.unwrap();

    assert_eq!(got[6], MsgType::Status as u8);
    let payload_len = u32::from_le_bytes(got[8..12].try_into().unwrap());
    assert_eq!(payload_len, dsp_rcp::STATUS_SIZE as u32);
    let uptime = u32::from_le_bytes(got[12..16].try_into().unwrap());
    assert_eq!(uptime, 7);

    drop(link.up);
    drop(client);
    link.task.await.unwrap().unwrap();
}
