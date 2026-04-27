import { CheckCircle, CloudUpload, File } from 'lucide-react'
import { useCallback, useState } from 'react'
import toast from 'react-hot-toast'
import { uploadFile } from '@/api/client'

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
	return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

export function UploadPage() {
	const [file, setFile] = useState<File | null>(null)
	const [uploading, setUploading] = useState(false)
	const [dragOver, setDragOver] = useState(false)
	const [result, setResult] = useState<{
		fileId: string
		sizeBytes: number
		checksum: string
	} | null>(null)

	const handleDrop = useCallback((e: React.DragEvent) => {
		e.preventDefault()
		setDragOver(false)
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
		<div className="max-w-2xl mx-auto space-y-8">
			<div>
				<h1 className="text-2xl font-bold text-gray-900">Upload File</h1>
				<p className="mt-1 text-sm text-muted">
					Upload files up to 100 MB. They are chunked and checksummed with
					SHA-256.
				</p>
			</div>

			<label
				onDrop={handleDrop}
				onDragOver={(e) => {
					e.preventDefault()
					setDragOver(true)
				}}
				onDragLeave={() => setDragOver(false)}
				className={`relative flex flex-col items-center justify-center rounded-2xl border-2 border-dashed p-16 cursor-pointer transition-all ${
					dragOver
						? 'border-primary bg-primary-light scale-[1.01]'
						: 'border-border bg-surface hover:border-primary/50 hover:bg-primary-light/50'
				}`}
			>
				<CloudUpload
					className={`w-14 h-14 mb-4 transition-colors ${dragOver ? 'text-primary' : 'text-gray-300'}`}
				/>
				<p className="text-base font-medium text-gray-700">
					Drag and drop a file here
				</p>
				<p className="mt-1 text-sm text-muted">or click to browse</p>
				<input
					type="file"
					className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
					onChange={(e) => setFile(e.target.files?.[0] ?? null)}
				/>
			</label>

			{file && (
				<div className="flex items-center gap-4 p-4 bg-surface rounded-xl border border-border shadow-sm">
					<div className="flex-shrink-0 w-10 h-10 rounded-lg bg-primary-light flex items-center justify-center">
						<File className="w-5 h-5 text-primary" />
					</div>
					<div className="flex-1 min-w-0">
						<p className="font-medium text-sm text-gray-900 truncate">
							{file.name}
						</p>
						<p className="text-xs text-muted">{formatBytes(file.size)}</p>
					</div>
					<button
						type="button"
						onClick={handleUpload}
						disabled={uploading}
						className={`px-5 py-2 rounded-lg text-sm font-medium text-white transition-all ${
							uploading
								? 'bg-primary/60 cursor-not-allowed'
								: 'bg-primary hover:bg-primary-hover shadow-sm hover:shadow'
						}`}
					>
						{uploading ? (
							<span className="flex items-center gap-2">
								<span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
								Uploading...
							</span>
						) : (
							'Upload'
						)}
					</button>
				</div>
			)}

			{result && (
				<div className="p-5 bg-success-light rounded-xl border border-success/20">
					<div className="flex items-start gap-3">
						<CheckCircle className="w-5 h-5 text-success mt-0.5 flex-shrink-0" />
						<div className="space-y-1">
							<p className="font-semibold text-gray-900">Upload complete</p>
							<div className="text-sm text-muted space-y-0.5">
								<p>
									<span className="text-gray-600">ID:</span>{' '}
									<code className="font-mono text-xs bg-white px-1.5 py-0.5 rounded">
										{result.fileId}
									</code>
								</p>
								<p>
									<span className="text-gray-600">Size:</span>{' '}
									{formatBytes(result.sizeBytes)}
								</p>
								<p>
									<span className="text-gray-600">SHA-256:</span>{' '}
									<code className="font-mono text-xs">
										{result.checksum.slice(0, 24)}...
									</code>
								</p>
							</div>
						</div>
					</div>
				</div>
			)}
		</div>
	)
}
