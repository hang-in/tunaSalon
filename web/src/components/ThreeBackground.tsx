import { useEffect, useRef } from "react";
import * as THREE from "three";

interface ThreeBackgroundProps {
  intensities: Record<string, number>;
  visible: boolean;
}

export function ThreeBackground({ intensities, visible }: ThreeBackgroundProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<THREE.WebGLRenderer | null>(null);
  const sceneRef = useRef<THREE.Scene | null>(null);
  const lightsRef = useRef<THREE.PointLight[]>([]);
  const meshRef = useRef<THREE.Mesh | null>(null);
  const frameRef = useRef(0);

  useEffect(() => {
    if (!containerRef.current) return;

    const container = containerRef.current;
    const w = window.innerWidth;
    const h = window.innerHeight;

    // Scene
    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#151515");
    sceneRef.current = scene;

    // Camera
    const camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 100);
    camera.position.z = 5;

    // Renderer
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.domElement.style.width = "100%";
    renderer.domElement.style.height = "100%";
    renderer.domElement.style.display = "block";
    container.appendChild(renderer.domElement);
    rendererRef.current = renderer;

    // Ambient
    scene.add(new THREE.AmbientLight("#303030", 0.6));

    // Point lights — one per persona
    const colors = ["#D9645A", "#8ABF9F", "#A89FCC"];
    const positions = [
      new THREE.Vector3(-2, 1.5, 2),
      new THREE.Vector3(2, 1, 2),
      new THREE.Vector3(0, -1.5, 2),
    ];
    const lights: THREE.PointLight[] = [];

    for (let i = 0; i < 3; i++) {
      const light = new THREE.PointLight(colors[i], 0, 8);
      light.position.copy(positions[i]);
      scene.add(light);
      lights.push(light);
    }
    lightsRef.current = lights;

    // Central abstract form — a smoothed cube
    const geo = new THREE.BoxGeometry(1.2, 1.2, 1.2);
    const mat = new THREE.MeshStandardMaterial({
      color: "#E5A44A",
      metalness: 0.6,
      roughness: 0.25,
      transparent: true,
      opacity: 0.6,
    });
    const mesh = new THREE.Mesh(geo, mat);
    scene.add(mesh);
    meshRef.current = mesh;

    // Add wireframe overlay
    const wireGeo = new THREE.EdgesGeometry(geo);
    const wireMat = new THREE.LineBasicMaterial({ color: "#E5A44A", transparent: true, opacity: 0.15 });
    const wireframe = new THREE.LineSegments(wireGeo, wireMat);
    mesh.add(wireframe);

    // Animation loop
    let running = true;
    const animate = () => {
      if (!running) return;
      frameRef.current = requestAnimationFrame(animate);

      mesh.rotation.x += 0.003;
      mesh.rotation.y += 0.005;

      // Map intensities to light brightness
      const ids = ["friend", "realist", "summarizer"];
      for (let i = 0; i < 3; i++) {
        const val = intensities[ids[i]] || 0;
        lights[i].intensity = val * 2.5;
      }

      renderer.render(scene, camera);
    };
    animate();

    // Resize handler
    const onResize = () => {
      const nw = window.innerWidth;
      const nh = window.innerHeight;
      camera.aspect = nw / nh;
      camera.updateProjectionMatrix();
      renderer.setSize(nw, nh);
    };
    window.addEventListener("resize", onResize);

    return () => {
      running = false;
      cancelAnimationFrame(frameRef.current);
      window.removeEventListener("resize", onResize);
      renderer.dispose();
      geo.dispose();
      mat.dispose();
      wireGeo.dispose();
      wireMat.dispose();
      if (container.contains(renderer.domElement)) {
        container.removeChild(renderer.domElement);
      }
    };
  }, []);

  // Update light intensities each frame from prop
  useEffect(() => {
    const ids = ["friend", "realist", "summarizer"];
    for (let i = 0; i < 3; i++) {
      const val = intensities[ids[i]] || 0;
      if (lightsRef.current[i]) {
        lightsRef.current[i].intensity = val * 2.5;
      }
    }
  }, [intensities]);

  return (
    <div
      ref={containerRef}
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        width: "100%",
        height: "100%",
        zIndex: 0,
        opacity: visible ? 0.35 : 0,
        transition: "opacity 1.5s ease",
        pointerEvents: "none",
      }}
    />
  );
}
