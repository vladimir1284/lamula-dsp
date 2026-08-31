# LAMULA DRx — el plan vive en el repositorio del DRx

Este sitio publicaba una copia íntegra del project plan de 34 semanas del LAMULA DRx. **Ya no.** El
proyecto DRx se escindió a su propio repositorio y a su propio sitio de documentación, y mientras
existieran dos copias vivas del mismo documento, un cambio en una no se reflejaba en la otra.

**La copia canónica es la del sitio del DRx:**

- [Project Plan ZU9 (34 semanas)](https://lamula-drx-docs.pages.dev/referencia/project-plan-zu9/) —
  el mismo documento, íntegro y sin editar.
- [Documentación del LAMULA DRx](https://lamula-drx-docs.pages.dev/) — contexto, decisiones
  congeladas, hallazgos abiertos y el plan de fases Z0–Z5, que ejecuta la parte construible de ese
  plan sobre una ZedBoard mientras no exista el hardware ZU9.

Esta página se conserva en lugar de borrarse para no romper los enlaces que ya apuntan aquí.

## Qué le importa a este proyecto del DRx

La única interfaz entre los dos es el **contrato DRx↔DSP**: rayos de I/Q diezmada y estado suben al
DSP, y configuración y control bajan, originados en el RCP y retransmitidos por el DSP. El DRx no
habla con el RCP ni con ORPG.

La versión v0.1 del esquema está congelada y su fuente única vive en el repositorio del DRx, que
genera desde ella las implementaciones de C, Rust y Python. La de Rust es la de este lado, y el test
de layout de esa implementación le toca a este proyecto: el repositorio del DRx no tiene toolchain
de Rust y solo comprueba que las constantes de tamaño coinciden.
