import SwiftUI
import UIKit

/// UIKit arbitration layer: pinch > pan > swipe > long press > double tap > zone tap.
struct TouchGestureBridge: UIViewRepresentable {
    var pinchEnabled = true
    var panEnabled = true
    var swipeEnabled = false
    var canPan: () -> Bool = { true }
    var canSwipe: () -> Bool = { true }
    var onPinch: (CGFloat) -> Void
    var onPan: (CGSize, Bool) -> Void
    var onSwipe: (UISwipeGestureRecognizer.Direction) -> Void
    var onLongPress: () -> Void
    var onDoubleTap: () -> Void
    var onZoneTap: (Int, Int) -> Void
    var onGenerationChanged: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> UIView {
        let view = UIView(frame: .zero)
        view.backgroundColor = .clear
        view.isOpaque = false
        let pinch = UIPinchGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pinch(_:)))
        let pan = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pan(_:)))
        pan.minimumNumberOfTouches = 1
        let swipe = UISwipeGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.swipe(_:)))
        swipe.direction = [.left, .right]
        let long = UILongPressGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.longPress(_:)))
        let double = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.doubleTap(_:)))
        double.numberOfTapsRequired = 2
        let single = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.singleTap(_:)))
        single.require(toFail: double)
        single.require(toFail: long)
        long.minimumPressDuration = 0.45
        [pinch, pan, swipe, long, double, single].forEach { view.addGestureRecognizer($0) }
        pinch.delegate = context.coordinator; pan.delegate = context.coordinator; swipe.delegate = context.coordinator
        long.delegate = context.coordinator; double.delegate = context.coordinator; single.delegate = context.coordinator
        return view
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        context.coordinator.parent = self
        let size = uiView.bounds.size
        if context.coordinator.lastSize != size {
            context.coordinator.lastSize = size
            context.coordinator.generation &+= 1
            DispatchQueue.main.async {
                self.onGenerationChanged()
            }
        }
    }

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var parent: TouchGestureBridge
        var generation = 0
        var lastSize: CGSize = .zero
        init(_ parent: TouchGestureBridge) { self.parent = parent }

        @objc func pinch(_ recognizer: UIPinchGestureRecognizer) {
            guard parent.pinchEnabled else { return }
            parent.onPinch(recognizer.scale)
            recognizer.scale = 1
        }
        @objc func pan(_ recognizer: UIPanGestureRecognizer) {
            let translation = recognizer.translation(in: recognizer.view)
            guard parent.panEnabled else { return }
            parent.onPan(CGSize(width: translation.x, height: translation.y), recognizer.state == .ended || recognizer.state == .cancelled)
            recognizer.setTranslation(.zero, in: recognizer.view)
        }
        @objc func swipe(_ recognizer: UISwipeGestureRecognizer) {
            guard parent.swipeEnabled, parent.canSwipe() else { return }
            parent.onSwipe(recognizer.direction)
        }
        @objc func longPress(_ recognizer: UILongPressGestureRecognizer) { if recognizer.state == .began { parent.onLongPress() } }
        @objc func doubleTap(_ recognizer: UITapGestureRecognizer) { if recognizer.state == .ended { parent.onDoubleTap() } }
        @objc func singleTap(_ recognizer: UITapGestureRecognizer) {
            guard let view = recognizer.view else { return }
            let point = recognizer.location(in: view)
            guard let zone = TouchZoneResolver.zone(at: point, in: view.bounds.size) else { return }
            parent.onZoneTap(zone.row, zone.column)
        }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
            gestureRecognizer is UIPinchGestureRecognizer && otherGestureRecognizer is UIPanGestureRecognizer
        }

        func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
            switch gestureRecognizer {
            case is UIPinchGestureRecognizer:
                return parent.pinchEnabled
            case is UIPanGestureRecognizer:
                return parent.panEnabled && parent.canPan()
            case is UISwipeGestureRecognizer:
                return parent.swipeEnabled && parent.canSwipe()
            default:
                return true
            }
        }
    }
}
