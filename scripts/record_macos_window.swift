import AVFoundation
import AppKit
import Foundation
import ScreenCaptureKit

struct Arguments {
    let applicationName: String
    let outputPath: String
    let durationSeconds: Double

    static func parse() throws -> Arguments {
        var applicationName = "Saccade"
        var outputPath: String?
        var durationSeconds = 24.0
        var index = 1

        while index < CommandLine.arguments.count {
            let argument = CommandLine.arguments[index]
            guard index + 1 < CommandLine.arguments.count else {
                throw RecorderError.usage("missing value for \(argument)")
            }
            let value = CommandLine.arguments[index + 1]
            switch argument {
            case "--application":
                applicationName = value
            case "--output":
                outputPath = value
            case "--duration":
                guard let parsed = Double(value), parsed > 0 else {
                    throw RecorderError.usage("--duration must be greater than zero")
                }
                durationSeconds = parsed
            default:
                throw RecorderError.usage("unknown argument: \(argument)")
            }
            index += 2
        }

        guard let outputPath else {
            throw RecorderError.usage("--output is required")
        }
        return Arguments(
            applicationName: applicationName,
            outputPath: outputPath,
            durationSeconds: durationSeconds
        )
    }
}

enum RecorderError: Error, CustomStringConvertible {
    case usage(String)
    case windowNotFound(String)
    case displayNotFound
    case outputExists(String)

    var description: String {
        switch self {
        case .usage(let message):
            return "\(message)\nusage: record_macos_window.swift --application Saccade --output demo.mp4 [--duration 24]"
        case .windowNotFound(let name):
            return "no visible window found for application matching \(name)"
        case .displayNotFound:
            return "no capturable display contains the selected window; grant Screen & System Audio Recording to the Codex or terminal host, then restart it"
        case .outputExists(let path):
            return "refusing to overwrite existing output: \(path)"
        }
    }
}

final class FrameWriter: NSObject, SCStreamOutput, SCStreamDelegate {
    let sampleQueue = DispatchQueue(label: "ai.nanlogic.saccade.window-recorder")
    private let assetWriter: AVAssetWriter
    private let videoInput: AVAssetWriterInput
    private let pixelBufferAdaptor: AVAssetWriterInputPixelBufferAdaptor
    private(set) var failure: Error?
    private(set) var appendedFrames = 0

    init(outputURL: URL, width: Int, height: Int) throws {
        assetWriter = try AVAssetWriter(outputURL: outputURL, fileType: .mp4)
        let compression: [String: Any] = [
            AVVideoAverageBitRateKey: 8_000_000,
            AVVideoExpectedSourceFrameRateKey: 30,
            AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
        ]
        let input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: width,
                AVVideoHeightKey: height,
                AVVideoCompressionPropertiesKey: compression,
            ]
        )
        input.expectsMediaDataInRealTime = true
        guard assetWriter.canAdd(input) else {
            throw NSError(
                domain: "SaccadeWindowRecorder",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "AVAssetWriter rejected the video input"]
            )
        }
        assetWriter.add(input)
        videoInput = input
        pixelBufferAdaptor = AVAssetWriterInputPixelBufferAdaptor(
            assetWriterInput: input,
            sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height,
            ]
        )
        super.init()
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        guard outputType == .screen, sampleBuffer.isValid, CMSampleBufferDataIsReady(sampleBuffer) else {
            return
        }
        if assetWriter.status == .unknown {
            guard assetWriter.startWriting() else {
                failure = assetWriter.error
                return
            }
            assetWriter.startSession(atSourceTime: sampleBuffer.presentationTimeStamp)
            print("recording_started")
            let hasImageBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) != nil
            if !hasImageBuffer {
                fputs("recording_waiting_for_first_pixel_frame\n", stderr)
                fflush(stderr)
            }
            fflush(stdout)
        }
        guard assetWriter.status == .writing else {
            failure = assetWriter.error
            return
        }
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
            return
        }
        if pixelBufferAdaptor.append(pixelBuffer, withPresentationTime: sampleBuffer.presentationTimeStamp) {
            appendedFrames += 1
        } else {
            failure = assetWriter.error
            fputs("append_failed: \(String(describing: assetWriter.error))\n", stderr)
            fflush(stderr)
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        failure = error
        fputs("stream_failed: \(error)\n", stderr)
        fflush(stderr)
    }

    func finish() async throws {
        sampleQueue.sync {
            videoInput.markAsFinished()
        }
        await withCheckedContinuation { continuation in
            assetWriter.finishWriting {
                continuation.resume()
            }
        }
        if let failure {
            throw failure
        }
        guard appendedFrames > 0, assetWriter.status == .completed else {
            throw assetWriter.error ?? NSError(
                domain: "SaccadeWindowRecorder",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "recording completed without video frames"]
            )
        }
        print("recording_finished frames=\(appendedFrames)")
        fflush(stdout)
    }
}

@main
struct WindowRecorder {
    static func main() async {
        do {
            _ = NSApplication.shared
            let arguments = try Arguments.parse()
            let outputURL = URL(fileURLWithPath: arguments.outputPath)
            guard !FileManager.default.fileExists(atPath: outputURL.path) else {
                throw RecorderError.outputExists(outputURL.path)
            }

            let content = try await SCShareableContent.current
            let requestedName = arguments.applicationName.lowercased()
            guard let window = content.windows.first(where: { candidate in
                let owner = candidate.owningApplication?.applicationName.lowercased() ?? ""
                return owner.contains(requestedName) && candidate.frame.width > 100 && candidate.frame.height > 100
            }) else {
                throw RecorderError.windowNotFound(arguments.applicationName)
            }
            guard let display = content.displays.first(where: { candidate in
                candidate.frame.intersects(window.frame)
            }) else {
                throw RecorderError.displayNotFound
            }

            let streamConfiguration = SCStreamConfiguration()
            streamConfiguration.width = max(2, (Int(window.frame.width) / 2) * 2)
            streamConfiguration.height = max(2, (Int(window.frame.height) / 2) * 2)
            streamConfiguration.minimumFrameInterval = CMTime(value: 1, timescale: 30)
            streamConfiguration.pixelFormat = kCVPixelFormatType_32BGRA
            streamConfiguration.queueDepth = 6
            streamConfiguration.showsCursor = true
            streamConfiguration.showMouseClicks = true
            streamConfiguration.capturesAudio = false
            streamConfiguration.sourceRect = CGRect(
                x: window.frame.minX - display.frame.minX,
                y: window.frame.minY - display.frame.minY,
                width: window.frame.width,
                height: window.frame.height
            )

            let frameWriter = try FrameWriter(
                outputURL: outputURL,
                width: streamConfiguration.width,
                height: streamConfiguration.height
            )
            let filter = SCContentFilter(display: display, including: [window])
            let stream = SCStream(
                filter: filter,
                configuration: streamConfiguration,
                delegate: frameWriter
            )
            try stream.addStreamOutput(
                frameWriter,
                type: .screen,
                sampleHandlerQueue: frameWriter.sampleQueue
            )
            try await stream.startCapture()
            try await Task.sleep(for: .seconds(arguments.durationSeconds))
            try await stream.stopCapture()
            try await frameWriter.finish()
            print("output=\(outputURL.path)")
        } catch {
            fputs("error: \(error)\n", stderr)
            exit(1)
        }
    }
}
