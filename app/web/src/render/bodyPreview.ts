// The species designer's 3D preview: one animal with the same model and shader as the
// 3D world (leg length, tail plume from the ornament trait, coat shade), turning slowly.

import * as THREE from "three";
import { animalGeometry, shapeAnimals } from "./world3d";
import type { RGB } from "../util";

export interface BodyLook {
  /** Leg length index (×). */
  legs: number;
  /** Ornament size, 0–1. */
  ornament: number;
  /** Coat luminance, 0–1. */
  coat: number;
  color: RGB;
}

export class BodyPreview {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(30, 1, 0.1, 20);
  private time = { value: 0 };
  private mat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  private mesh: THREE.InstancedMesh;
  private shape = new THREE.InstancedBufferAttribute(new Float32Array(4), 4);
  private frame = 0;
  private start = performance.now();

  constructor(private canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
    shapeAnimals(this.mat, this.time);
    const geo = animalGeometry();
    geo.setAttribute("aShape", this.shape);
    this.mesh = new THREE.InstancedMesh(geo, this.mat, 1);
    this.mesh.setMatrixAt(0, new THREE.Matrix4());
    this.mesh.setColorAt(0, new THREE.Color(1, 1, 1));
    this.scene.add(this.mesh);
    const ground = new THREE.Mesh(new THREE.CircleGeometry(0.75, 40), new THREE.MeshLambertMaterial({ color: 0x8a9a6a }));
    ground.rotateX(-Math.PI / 2);
    this.scene.add(ground);
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x445533, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(2, 3, 1.5);
    this.scene.add(sun);
    this.camera.position.set(0, 0.9, 2.6);
    this.camera.lookAt(0, 0.4, 0);
    const tick = () => {
      this.frame = requestAnimationFrame(tick);
      this.draw();
    };
    this.frame = requestAnimationFrame(tick);
  }

  set(look: BodyLook) {
    // The same mapping as the 3D world (see `World3D`).
    const legs = Math.min(1.6, Math.max(0.65, Math.sqrt(look.legs)));
    const ornament = Math.min(1, look.ornament * 1.3);
    this.shape.setXYZW(0, legs, ornament, 0, 0);
    this.shape.needsUpdate = true;
    const c = new THREE.Color().setRGB(look.color[0] / 255, look.color[1] / 255, look.color[2] / 255, THREE.SRGBColorSpace);
    c.multiplyScalar(0.7 + 0.6 * look.coat);
    this.mesh.setColorAt(0, c);
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
  }

  private draw() {
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    if (w === 0 || h === 0) return;
    const size = this.renderer.getSize(new THREE.Vector2());
    if (size.x !== w || size.y !== h) {
      this.renderer.setSize(w, h, false);
      this.camera.aspect = w / h;
      this.camera.updateProjectionMatrix();
    }
    const t = (performance.now() - this.start) / 1000;
    this.mesh.rotation.y = 0.6 + t * 0.4;
    this.renderer.render(this.scene, this.camera);
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    this.mesh.geometry.dispose();
    this.mat.dispose();
    this.renderer.dispose();
  }
}
