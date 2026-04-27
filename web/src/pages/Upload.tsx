import { CheckCircle, Upload } from 'lucide-react'
import { useCallback, useState } from 'react'
import toast from 'react-hot-toast'
import { uploadFile } from '@/api/client'

export function UploadPage() {
	const [file, setFile] = useState<File | null>(null)
	const [uploading, setUploading] = useState(false)
	const [result, setResult] = useState<{
		fileId: string
		sizeBytes: number
		checksum: string
	} | null>(null)

	const handleDrop = useCallback((e: React.DragEvent) => {
		e.preventDefault()
		const dropped = e.dataTransfer.files[0]
		if (dropped) setFile(dropped)
	}, [])

	const handleUpload = async () => {
		if (!file) return
		setUploading(true)
		setResult(null)

		try {
			const resp = await uploadFile(file)
			setResult(resp)
			toast.success(`Uploaded: ${file.name}`)
		} catch (err) {
			toast.error(err instanceof Error ? err.message : 'Upload failed')
		} finally {
			setUploading(false)
		}
	}

	return (
		<div className="max-w-2xl mx-auto space-y-6">
			<h1 className="text-3xl font-bold">Upload File</h1>

			<label
				onDrop={handleDrop}
				onDragOver={(e) => e.preventDefault()}
				className="border-2 border-dashed border-base-300 rounded-xl p-12 text-center cursor-pointer hover:border-primary transition-colors block"
			>
				<Upload className="mx-auto mb-4 w-12 h-12 text-base-content/50" />
				<p className="text-lg">Drag and drop a file here</p>
				<p className="text-sm text-base-content/60 mt-1">or click to browse</p>
				<input
					type="file"
					className="absolute inset-0 opacity-0 cursor-pointer"
					onChange={(e) => setFile(e.target.files?.[0] ?? null)}
				/>
			</label>

			{file && (
				<div className="flex items-center gap-4 p-4 bg-base-200 rounded-lg">
					<span className="font-mono text-sm truncate flex-1">{file.name}</span>
					<span className="text-sm text-base-content/60">
						{(file.size / 1024).toFixed(1)} KB
					</span>
					<button
						type="button"
						onClick={handleUpload}
						disabled={uploading}
						className="btn btn-primary btn-sm"
					>
						{uploading ? 'Uploading...' : 'Upload'}
					</button>
				</div>
			)}

			{result && (
				<div className="alert alert-success">
					<CheckCircle className="w-5 h-5" />
					<div>
						<p className="font-bold">Upload complete</p>
						<p className="text-sm font-mono">ID: {result.fileId}</p>
						<p className="text-sm">
							{result.sizeBytes} bytes — SHA-256: {result.checksum.slice(0, 16)}
							…
						</p>
					</div>
				</div>
			)}
		</div>
	)
}
