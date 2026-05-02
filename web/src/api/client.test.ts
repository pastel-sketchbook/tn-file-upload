import { beforeEach, describe, expect, it, vi } from 'vitest'
import { deleteFile, downloadFile, listFiles, uploadFile } from './client'

function mockFetch(body: unknown, status = 200) {
	return vi.fn().mockResolvedValue({
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(body),
		text: () => Promise.resolve(JSON.stringify(body)),
		blob: () => Promise.resolve(new Blob(['data'])),
	})
}

beforeEach(() => {
	vi.restoreAllMocks()
})

describe('API client', () => {
	it('listFiles sends auth header and returns files', async () => {
		const files = [
			{
				fileId: '1',
				fileName: 'a.txt',
				contentType: 'text/plain',
				sizeBytes: 10,
				sha256Checksum: 'abc',
				uploadedAt: '2026-01-01',
			},
		]
		globalThis.fetch = mockFetch(files)

		const result = await listFiles()

		expect(result).toEqual(files)
		expect(globalThis.fetch).toHaveBeenCalledWith(
			'/api/files',
			expect.objectContaining({ headers: { 'x-auth-token': 'dev-token' } }),
		)
	})

	it('uploadFile sends file as multipart with auth header', async () => {
		const response = { fileId: '1', sizeBytes: 100, checksum: 'abc' }
		globalThis.fetch = mockFetch(response)

		const file = new File(['hello'], 'test.txt', { type: 'text/plain' })
		const result = await uploadFile(file)

		expect(result).toEqual(response)
		expect(globalThis.fetch).toHaveBeenCalledWith(
			'/api/upload',
			expect.objectContaining({
				method: 'POST',
				headers: { 'x-auth-token': 'dev-token' },
			}),
		)
	})

	it('uploadFile throws on non-ok response', async () => {
		globalThis.fetch = mockFetch({ error: 'too large' }, 413)

		const file = new File(['x'], 'big.bin')
		await expect(uploadFile(file)).rejects.toThrow()
	})

	it('deleteFile sends DELETE with auth header', async () => {
		globalThis.fetch = mockFetch(null, 204)

		await deleteFile('file-123')

		expect(globalThis.fetch).toHaveBeenCalledWith(
			'/api/files/file-123',
			expect.objectContaining({
				method: 'DELETE',
				headers: { 'x-auth-token': 'dev-token' },
			}),
		)
	})

	it('downloadFile returns blob with auth header', async () => {
		globalThis.fetch = mockFetch(null)

		const blob = await downloadFile('file-123')

		expect(blob).toBeInstanceOf(Blob)
		expect(globalThis.fetch).toHaveBeenCalledWith(
			'/api/files/file-123/download',
			expect.objectContaining({ headers: { 'x-auth-token': 'dev-token' } }),
		)
	})
})
