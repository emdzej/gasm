/**************************************************************************/
/*  http_client_gasm.cpp                                                  */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#include "http_client_gasm.h"

#include "core/io/json.h"
#include "core/object/class_db.h"

#include "gasm.h"

// Headers gasm:fetch refuses (browsers refuse them too: fetch() drops them). HTTPRequest
// and HTTPClient add some on their own (Accept-Encoding, User-Agent), so they are
// dropped here instead of failing the request. The runner decodes compressed bodies.
static bool dropped_header(const String &p_name) {
	static const char *names[] = { "accept-charset", "accept-encoding", "access-control-request-headers", "access-control-request-method",
		"connection", "content-length", "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
		"origin", "referer", "set-cookie", "te", "trailer", "transfer-encoding", "upgrade", "user-agent", "via" };
	String n = p_name.strip_edges().to_lower();
	if (n.begins_with("proxy-") || n.begins_with("sec-")) {
		return true;
	}
	for (const char *d : names) {
		if (n == d) {
			return true;
		}
	}
	return false;
}

void HTTPClientGasm::free_request() {
	if (req > 0) {
		gasm_fetch_close(req);
	}
	req = 0;
}

Error HTTPClientGasm::connect_to_host(const String &p_host, int p_port, Ref<TLSOptions> p_tls_options) {
	ERR_FAIL_COND_V(p_tls_options.is_valid() && p_tls_options->is_server(), ERR_INVALID_PARAMETER);
	close();
	port = p_port;
	use_tls = p_tls_options.is_valid();
	host = p_host;
	String lower = host.to_lower();
	if (lower.begins_with("http://")) {
		host = host.substr(7);
		use_tls = false;
	} else if (lower.begins_with("https://")) {
		host = host.substr(8);
		use_tls = true;
	}
	ERR_FAIL_COND_V(host.length() < HOST_MIN_LEN, ERR_INVALID_PARAMETER);
	if (port < 0) {
		port = use_tls ? PORT_HTTPS : PORT_HTTP;
	}
	// nothing to connect: the runner does it per request
	status = host.is_valid_ip_address() ? STATUS_CONNECTING : STATUS_RESOLVING;
	return OK;
}

void HTTPClientGasm::set_connection(const Ref<StreamPeer> &p_connection) {
	ERR_FAIL_MSG("HTTPClient's StreamPeer is not available on gasm (the runner makes the requests).");
}

Ref<StreamPeer> HTTPClientGasm::get_connection() const {
	ERR_FAIL_V_MSG(Ref<RefCounted>(), "HTTPClient's StreamPeer is not available on gasm (the runner makes the requests).");
}

Error HTTPClientGasm::request(Method p_method, const String &p_url, const Vector<String> &p_headers, const uint8_t *p_body, int p_body_len) {
	ERR_FAIL_INDEX_V(p_method, METHOD_MAX, ERR_INVALID_PARAMETER);
	ERR_FAIL_COND_V_MSG(p_method == METHOD_TRACE || p_method == METHOD_CONNECT, ERR_UNAVAILABLE, "HTTP methods TRACE and CONNECT are not supported on gasm.");
	ERR_FAIL_COND_V(status != STATUS_CONNECTED, ERR_INVALID_PARAMETER);
	ERR_FAIL_COND_V(host.is_empty() || port < 0, ERR_UNCONFIGURED);
	ERR_FAIL_COND_V(!p_url.begins_with("/"), ERR_INVALID_PARAMETER);
	Error err = verify_headers(p_headers);
	if (err) {
		return err;
	}
	ERR_FAIL_COND_V_MSG(gasm_has_str("gasm:fetch") != 1, ERR_UNAVAILABLE, "This gasm runner has no gasm:fetch (HTTP requests).");

	bool default_port = (use_tls && port == PORT_HTTPS) || (!use_tls && port == PORT_HTTP);
	String url = (use_tls ? "https://" : "http://") + host + (default_port ? String() : ":" + itos(port)) + p_url;
	Dictionary headers;
	for (const String &h : p_headers) {
		int colon = h.find(":");
		String name = h.substr(0, colon).strip_edges();
		if (colon > 0 && !dropped_header(name)) {
			headers[name] = h.substr(colon + 1).strip_edges();
		}
	}
	Dictionary desc;
	desc["method"] = _methods[p_method];
	desc["url"] = url;
	desc["headers"] = headers;
	CharString d = JSON::stringify(desc).utf8();
	free_request();
	response_code = 0;
	response_headers.clear();
	headers_read = false;
	req = gasm_fetch_request(d.ptr(), d.length(), p_body, p_body_len > 0 ? p_body_len : 0);
	if (req <= 0) {
		req = 0;
		status = STATUS_CONNECTION_ERROR; // refused (the runner's log says why)
		return ERR_CANT_CONNECT;
	}
	status = STATUS_REQUESTING;
	return OK;
}

void HTTPClientGasm::close() {
	free_request();
	host = "";
	port = -1;
	use_tls = false;
	status = STATUS_DISCONNECTED;
	response_code = 0;
	response_headers.clear();
	headers_read = false;
}

HTTPClientGasm::Status HTTPClientGasm::get_status() const {
	return status;
}

bool HTTPClientGasm::has_response() const {
	return !response_headers.is_empty();
}

bool HTTPClientGasm::is_response_chunked() const {
	return false; // the runner hands over the body as it arrives, already de-chunked
}

int HTTPClientGasm::get_response_code() const {
	return response_code;
}

Error HTTPClientGasm::get_response_headers(List<String> *r_response) {
	if (response_headers.is_empty()) {
		return ERR_INVALID_PARAMETER;
	}
	for (const String &h : response_headers) {
		r_response->push_back(h);
	}
	response_headers.clear();
	return OK;
}

int64_t HTTPClientGasm::get_response_body_length() const {
	return -1; // the runner decodes compressed bodies: the length is known only at the end
}

PackedByteArray HTTPClientGasm::read_response_body_chunk() {
	ERR_FAIL_COND_V(status != STATUS_BODY, PackedByteArray());
	PackedByteArray chunk;
	chunk.resize(read_limit);
	int32_t n = gasm_fetch_read(req, chunk.ptrw(), read_limit);
	if (n < 0) {
		chunk.clear();
		status = gasm_fetch_state(req) == GASM_FETCH_DONE ? STATUS_DISCONNECTED : STATUS_CONNECTION_ERROR;
		free_request();
		return chunk;
	}
	chunk.resize(n);
	return chunk;
}

void HTTPClientGasm::set_blocking_mode(bool p_enable) {
	ERR_FAIL_COND_MSG(p_enable, "HTTPClient's blocking mode is not available on gasm.");
}

bool HTTPClientGasm::is_blocking_mode_enabled() const {
	return false;
}

void HTTPClientGasm::set_read_chunk_size(int p_size) {
	ERR_FAIL_COND(p_size < 256 || p_size > (1 << 24));
	read_limit = p_size;
}

int HTTPClientGasm::get_read_chunk_size() const {
	return read_limit;
}

Error HTTPClientGasm::poll() {
	switch (status) {
		case STATUS_DISCONNECTED:
			return ERR_UNCONFIGURED;
		case STATUS_RESOLVING:
			status = STATUS_CONNECTING;
			return OK;
		case STATUS_CONNECTING:
			status = STATUS_CONNECTED;
			return OK;
		case STATUS_CONNECTED:
		case STATUS_BODY:
			return OK; // the body is read with read_response_body_chunk
		case STATUS_CONNECTION_ERROR:
			return ERR_CONNECTION_ERROR;
		case STATUS_REQUESTING: {
			uint32_t s = gasm_fetch_state(req);
			if (s == GASM_FETCH_PENDING) {
				return OK;
			}
			if (s == GASM_FETCH_FAILED) {
				status = STATUS_CONNECTION_ERROR;
				free_request();
				return ERR_CONNECTION_ERROR;
			}
			response_code = gasm_fetch_status(req);
			int32_t len = gasm_fetch_headers(req, nullptr, 0);
			if (len > 0) {
				CharString text;
				text.resize_uninitialized(len + 1);
				gasm_fetch_headers(req, text.ptrw(), len);
				text.ptrw()[len] = 0;
				for (const String &line : String::utf8(text.ptr(), len).split("\n", false)) {
					response_headers.push_back(line);
				}
			}
			if (response_headers.is_empty()) {
				response_headers.push_back("x-gasm-fetch: 1"); // has_response() needs one
			}
			status = STATUS_BODY;
			return OK;
		}
		default:
			ERR_FAIL_V(ERR_BUG);
	}
}

HTTPClient *HTTPClientGasm::_create_func(bool p_notify_postinitialize) {
	return static_cast<HTTPClient *>(ClassDB::creator<HTTPClientGasm>(p_notify_postinitialize));
}

HTTPClient *(*HTTPClient::_create)(bool p_notify_postinitialize) = HTTPClientGasm::_create_func;

HTTPClientGasm::HTTPClientGasm() {
}

HTTPClientGasm::~HTTPClientGasm() {
	free_request();
}
