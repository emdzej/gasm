/**************************************************************************/
/*  http_client_gasm.h                                                    */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#pragma once

#include "core/io/http_client.h"

// HTTPClient on gasm:fetch: the runner makes the request (TLS included), so
// HTTPRequest works unchanged, https:// too. Modeled on HTTPClientWeb: no
// StreamPeer, no blocking mode; a request progresses once per frame.
class HTTPClientGasm : public HTTPClient {
	GDSOFTCLASS(HTTPClientGasm, HTTPClient);

private:
	int32_t req = 0;
	Status status = STATUS_DISCONNECTED;
	int read_limit = 65536;

	String host;
	int port = -1;
	bool use_tls = false;

	int response_code = 0;
	Vector<String> response_headers;
	bool headers_read = false;

	void free_request();

public:
	static HTTPClient *_create_func(bool p_notify_postinitialize);

	Error request(Method p_method, const String &p_url, const Vector<String> &p_headers, const uint8_t *p_body, int p_body_size) override;

	Error connect_to_host(const String &p_host, int p_port = -1, Ref<TLSOptions> p_tls_options = Ref<TLSOptions>()) override;
	void set_connection(const Ref<StreamPeer> &p_connection) override;
	Ref<StreamPeer> get_connection() const override;
	void close() override;
	Status get_status() const override;
	bool has_response() const override;
	bool is_response_chunked() const override;
	int get_response_code() const override;
	Error get_response_headers(List<String> *r_response) override;
	int64_t get_response_body_length() const override;
	PackedByteArray read_response_body_chunk() override;
	void set_blocking_mode(bool p_enable) override;
	bool is_blocking_mode_enabled() const override;
	void set_read_chunk_size(int p_size) override;
	int get_read_chunk_size() const override;
	Error poll() override;
	HTTPClientGasm();
	~HTTPClientGasm();
};
