/**
 * Thin Connect protocol client for the file_upload.v1.FileUpload service.
 *
 * Uses Connect's unary JSON protocol over HTTP/1.1 for browser compatibility.
 * Client-streaming (Upload) and server-streaming (Download) use a REST-like
 * approach via a separate HTTP endpoint on the server, since gRPC-Web doesn't
 * support client-streaming natively.
 *
 * For the SPA, we'll add a small REST shim to the Rust server that wraps
 * the gRPC logic, or use unary upload (single chunk / multipart).
 */

const BASE_URL = '' // Same origin, proxied by Vite in dev

export interface FileMeta {
	fileId: string
	fileName: string
	contentType: string
	sizeBytes: number
	sha256Checksum: string
	uploadedAt: string
}

export interface UploadResult {
	fileId: string
	sizeBytes: number
	checksum: string
}

/**
 * Upload a file via the REST shim endpoint.
 * The Rust server exposes POST /api/upload that accepts multipart/form-data.
 */
export async function uploadFile(file: File): Promise<UploadResult> {
	const formData = new FormData()
	formData.append('file', file)

	const resp = await fetch(`${BASE_URL}/api/upload`, {
		method: 'POST',
		body: formData,
	})

	if (!resp.ok) {
		const text = await resp.text()
		throw new Error(text || `Upload failed: ${resp.status}`)
	}

	return resp.json()
}

/**
 * List files via GET /api/files.
 */
export async function listFiles(): Promise<FileMeta[]> {
	const resp = await fetch(`${BASE_URL}/api/files`)
	if (!resp.ok) throw new Error('Failed to list files')
	return resp.json()
}

/**
 * Delete a file via DELETE /api/files/:id.
 */
export async function deleteFile(fileId: string): Promise<void> {
	const resp = await fetch(`${BASE_URL}/api/files/${fileId}`, {
		method: 'DELETE',
	})
	if (!resp.ok) throw new Error('Failed to delete file')
}

/**
 * Download a file via GET /api/files/:id/download.
 */
export async function downloadFile(fileId: string): Promise<Blob> {
	const resp = await fetch(`${BASE_URL}/api/files/${fileId}/download`)
	if (!resp.ok) throw new Error('Failed to download file')
	return resp.blob()
}
