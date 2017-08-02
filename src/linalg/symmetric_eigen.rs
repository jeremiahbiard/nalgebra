use num_complex::Complex;
use std::ops::MulAssign;

use alga::general::Real;
use core::{MatrixN, VectorN, DefaultAllocator, Matrix2, Vector2};
use dimension::{Dim, DimSub, DimDiff, U1, U2};
use allocator::Allocator;

use linalg::givens;
use linalg::SymmetricTridiagonal;
use geometry::UnitComplex;


/// The eigendecomposition of a symmetric matrix.
pub struct SymmetricEigen<N: Real, D: Dim>
    where DefaultAllocator: Allocator<N, D, D> +
                            Allocator<N, D> {
    /// The eigenvectors of the decomposed matrix.
    pub eigenvectors: MatrixN<N, D>,

    /// The unsorted eigenvalues of the decomposed matrix.
    pub eigenvalues: VectorN<N, D>
}

impl<N: Real, D: Dim> SymmetricEigen<N, D>
    where DefaultAllocator: Allocator<N, D, D> +
                            Allocator<N, D> {
    /// Computes the eigendecomposition of the given symmetric matrix.
    ///
    /// Only the lower-triangular and diagonal parts of `m` are read.
    pub fn new(m: MatrixN<N, D>) -> Self
        where D: DimSub<U1>,
              DefaultAllocator: Allocator<N, DimDiff<D, U1>> {

        Self::try_new(m, N::default_epsilon(), 0).unwrap()
    }

    /// Computes the eigendecomposition of the given symmetric matrix with user-specified
    /// convergence parameters.
    ///
    /// Only the lower-triangular and diagonal parts of `m` are read.
    ///
    /// # Arguments
    ///
    /// * `eps`       − tolerence used to determine when a value converged to 0.
    /// * `max_niter` − maximum total number of iterations performed by the algorithm. If this
    /// number of iteration is exceeded, `None` is returned. If `niter == 0`, then the algorithm
    /// continues indefinitely until convergence.
    pub fn try_new(mut m: MatrixN<N, D>, eps: N, max_niter: usize) -> Option<Self>
        where D: DimSub<U1>,
              DefaultAllocator: Allocator<N, DimDiff<D, U1>> {

        assert!(m.is_square(), "Unable to compute the eigendecomposition of a non-square matrix.");
        let dim = m.nrows();

        let m_amax = m.amax();

        if !m_amax.is_zero() {
            m /= m_amax;
        }

        let (mut q, mut diag, mut off_diag) = SymmetricTridiagonal::new(m).unpack();

        if dim == 1 {
            diag *= m_amax;

            return Some(SymmetricEigen {
                eigenvectors: q,
                eigenvalues:  diag
            });
        }

        let mut niter = 0;
        let (mut start, mut end) = Self::delimit_subproblem(&diag, &mut off_diag, dim - 1, eps);

        while end != start {
            let subdim = end - start + 1;

            if subdim > 2 {
                let m = end - 1;
                let n = end;

                let mut v = Vector2::new(
                    diag[start] - wilkinson_shift(diag[m], diag[n], off_diag[m]),
                    off_diag[start]);


                for i in start .. n {
                    let j = i + 1;

                    if let Some((rot, norm)) = givens::cancel_y(&v) {
                        if i > start {
                            // Not the first iteration.
                            off_diag[i - 1] = norm;
                        }

                        let mii = diag[i];
                        let mjj = diag[j];
                        let mij = off_diag[i];

                        let cc = rot.cos_angle() * rot.cos_angle();
                        let ss = rot.sin_angle() * rot.sin_angle();
                        let cs = rot.cos_angle() * rot.sin_angle();

                        let b = cs * ::convert(2.0) * mij;

                        diag[i]     = (cc * mii + ss * mjj) - b;
                        diag[j]     = (ss * mii + cc * mjj) + b;
                        off_diag[i] = cs * (mii - mjj) + mij * (cc - ss);

                        if i != n - 1 {
                            v.x = off_diag[i];
                            v.y = -rot.sin_angle() * off_diag[i + 1];
                            off_diag[i + 1] *= rot.cos_angle();
                        }

                        rot.inverse().rotate_rows(&mut q.fixed_columns_mut::<U2>(i));
                    }
                    else {
                        break;
                    }
                }

                if off_diag[m].abs() <= eps * (diag[m].abs() + diag[n].abs()) {
                    end -= 1;
                }
            }
            else if subdim == 2 {
                let m = Matrix2::new(diag[start],     off_diag[start],
                                     off_diag[start], diag[start + 1]);
                let eigvals = m.eigenvalues().unwrap();
                let basis   = Vector2::new(eigvals.x - diag[start + 1], off_diag[start]);

                diag[start + 0] = eigvals[0];
                diag[start + 1] = eigvals[1];

                if let Some(basis) = basis.try_normalize(eps) {
                    let rot = UnitComplex::new_unchecked(Complex::new(basis.x, basis.y));
                    rot.rotate_rows(&mut q.fixed_columns_mut::<U2>(start));
                }

                end -= 1;
            }

            // Re-delimit the suproblem in case some decoupling occured.
            let sub = Self::delimit_subproblem(&diag, &mut off_diag, end, eps);

            start = sub.0;
            end   = sub.1;

            niter += 1;
            if niter == max_niter {
                return None;
            }
        }

        diag *= m_amax;

        // Solve the remaining 2x2 subproblem.

        Some(SymmetricEigen {
            eigenvectors: q,
            eigenvalues:  diag
        })
    }

    fn delimit_subproblem(diag:     &VectorN<N, D>,
                          off_diag: &mut VectorN<N, DimDiff<D, U1>>,
                          end:      usize,
                          eps:      N)
                          -> (usize, usize)
        where D: DimSub<U1>,
              DefaultAllocator: Allocator<N, DimDiff<D, U1>> {

        let mut n = end;

        while n > 0 {
            let m = n - 1;

            if off_diag[m].abs() > eps * (diag[n].abs() + diag[m].abs()) {
                break;
            }

            n -= 1;
        }

        if n == 0 {
            return (0, 0);
        }

        let mut new_start = n - 1;
        while new_start > 0 {
            let m = new_start - 1;

            if off_diag[m].is_zero() ||
               off_diag[m].abs() <= eps * (diag[new_start].abs() + diag[m].abs()) {
                off_diag[m] = N::zero();
                break;
            }

            new_start -= 1;
        }

        (new_start, n)
    }

    /// Rebuild the original matrix.
    ///
    /// This is useful if some of the eigenvalues have been manually modified.
    pub fn recompose(&self) -> MatrixN<N, D> {
        let mut u_t = self.eigenvectors.clone();
        for i in 0 .. self.eigenvalues.len() {
            let val = self.eigenvalues[i];
            u_t.column_mut(i).mul_assign(val);
        }
        u_t.transpose_mut();
        &self.eigenvectors * u_t
    }
}

/// Computes the wilkinson shift, i.e., the 2x2 symmetric matrix eigenvalue to its tailing
/// component `tnn`.
///
/// The inputs are interpreted as the 2x2 matrix:
///     tmm  tmn
///     tmn  tnn
pub fn wilkinson_shift<N: Real>(tmm: N, tnn: N, tmn: N) -> N {
    let sq_tmn = tmn * tmn;
    if !sq_tmn.is_zero() {
        // We have the guarantee thet the denominator won't be zero.
        let d = (tmm - tnn) * ::convert(0.5);
        tnn - sq_tmn / (d + d.signum() * (d * d + sq_tmn).sqrt())
    }
    else {
        tnn
    }
}

#[cfg(test)]
mod test {
    use core::Matrix2;

    fn expected_shift(m: Matrix2<f64>) -> f64 {
        let vals = m.eigenvalues().unwrap();

        if (vals.x - m.m22).abs() < (vals.y - m.m22).abs() {
            vals.x
        } else {
            vals.y
        }
    }

    #[test]
    fn wilkinson_shift_random() {
        for _ in 0 .. 1000 {
            let m = Matrix2::new_random();
            let m = m * m.transpose();

            let expected = expected_shift(m);
            let computed = super::wilkinson_shift(m.m11, m.m22, m.m12);
            println!("{} {}", expected, computed);
            assert!(relative_eq!(expected, computed, epsilon = 1.0e-7));
        }
    }

    #[test]
    fn wilkinson_shift_zero() {
        let m = Matrix2::new(0.0, 0.0,
                             0.0, 0.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }


    #[test]
    fn wilkinson_shift_zero_diagonal() {
        let m = Matrix2::new(0.0, 42.0,
                             42.0, 0.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }

    #[test]
    fn wilkinson_shift_zero_off_diagonal() {
        let m = Matrix2::new(42.0, 0.0,
                             0.0, 64.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }

    #[test]
    fn wilkinson_shift_zero_trace() {
        let m = Matrix2::new(42.0, 20.0,
                             20.0, -42.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }

    #[test]
    fn wilkinson_shift_zero_diag_diff_and_zero_off_diagonal() {
        let m = Matrix2::new(42.0, 0.0,
                             0.0,  42.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }

    #[test]
    fn wilkinson_shift_zero_det() {
        let m = Matrix2::new(2.0, 4.0,
                             4.0, 8.0);
        assert!(relative_eq!(expected_shift(m), super::wilkinson_shift(m.m11, m.m22, m.m12)));
    }
}