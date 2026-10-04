/**************************************************************************/
/*  ip_gasm.h                                                             */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#pragma once

#include "core/io/ip.h"

// No sockets on gasm (gasm:net is messages): no host names, no interfaces.
class IPGasm : public IP {
	GDCLASS(IPGasm, IP);

	void _resolve_hostname(List<IPAddress> &r_addresses, const String &p_hostname, Type p_type = TYPE_ANY) const override {}

	static IP *_create_gasm() { return memnew(IPGasm); }

public:
	void get_local_interfaces(HashMap<String, Interface_Info> *r_interfaces) const override {}

	static void make_default() { _create = _create_gasm; }
};
