use std::env;
use std::net::UdpSocket;

pub fn move_obj<T>(o: &mut T)->T
	where T: Default {
	let mut o2 = T::default();
	std::mem::swap(o, &mut o2);
	o2
}

pub fn env_as_bool(name: &str)->bool {
	match env::var(name) {
		Ok(val) => {
			val == "1" || val == "true"
		},
		Err(_) => false
	}
}

pub fn select_val<T>(condi: bool, val1: T, val2: T)->T {
	if condi {
		val1
	} else {
		val2
	}
}

// only used when debugging, as a temp logger
#[allow(dead_code)]
pub(crate) fn udp_msg(ipv4: &str, port: u16, msg: &str) {
	if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
		let target = format!("{}:{}", ipv4, port);
		let _ = socket.send_to(msg.as_bytes(), &target);
	}
}
