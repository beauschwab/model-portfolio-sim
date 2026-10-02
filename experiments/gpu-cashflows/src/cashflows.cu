// Port of quant.rs mortgage_step / deposits, with ascending path sums.
// One block per instrument. Each lane owns up to eight independent paths.
// Only current-month path contributions are retained; no path x book x time cube.
typedef unsigned long long U;
__device__ double logistic(double z, bool rational) {
    if (!rational) return z > 30.0 ? 1.0 : (z < -30.0 ? 0.0 : 1.0 / (1.0 + exp(-z)));
    double x = z * 0.5;
    if (x >= 6.0) return 1.0;
    if (x <= -6.0) return 0.0;
    double x2 = x*x;
    double v = x*(135135.0+x2*(17325.0+x2*(378.0+x2))) /
                    (135135.0+x2*(62370.0+x2*(3150.0+28.0*x2)));
    return 0.5 + 0.5 * fmin(1.0, fmax(-1.0,v));
}
__device__ double lookup(double u, const double* v, int size) {
    if (u <= 0.0) return v[0];
    int i = (int)u;
    if (i >= size-1) return v[size-1];
    double f = u - i;
    return v[i]*(1.0-f) + v[i+1]*f;
}
__device__ double spline(double x, const double* knots, const double* c, int size) {
    x = fmin(knots[size-1],fmax(knots[0],x));
    int i = 0;
    for (int j=0;j<size-1;j++) if (x>=knots[j]) i=j;
    double dx=x-knots[i];
    int o=4*i;
    return ((c[o]*dx+c[o+1])*dx+c[o+2])*dx+c[o+3];
}
__device__ double integer_power(double x, int exponent) {
    unsigned int n = exponent < 0 ? -exponent : exponent;
    double y = 1.0;
    while (n) { if (n & 1) y *= x; n >>= 1; if (n) x *= x; }
    return exponent < 0 ? 1.0/y : y;
}
extern "C" __global__ void cashflows(const double* a, const U* o, double* scratch,
                                      double* out, int product, int paths, int months, int count) {
    int s=blockIdx.x, lane=threadIdx.x;
    const double* v[25];
    for (int i=0;i<(product==4?25:19);i++) v[i]=a+o[i];
    double bal[8], burn[8], q[8];
    bool active[8];
    for (int j=0;j<8;j++) {
        int p=lane+j*256;
        active[j]=p<paths; bal[j]=1.0; burn[j]=1.0;
        q[j]=product==4 ? integer_power(1.0+v[13][s]/12.0,-(int)v[15][s]) : 0.0;
    }
    U span=(U)count*months;
    double* tmp=scratch+(U)s*paths*3;
    for (int m=0;m<months;m++) {
        for (int j=0;j<8;j++) {
            int p=lane+j*256;
            if (p>=paths) continue;
            double cf=0.0, intr=0.0, principal=0.0;
            if (active[j]) {
                U pm=(U)m*paths+p;
                if (product==4) {
                    const double* pp=v[6];
                    double r=v[13][s]/12.0;
                    double inc=v[13][s]-v[0][pm];
                    double refi=pp[0]*logistic(pp[1]+pp[2]*inc,v[24][0]!=0.0)*burn[j];
                    double lock=pp[7]+(1.0-pp[7])*logistic(pp[8]*inc,v[24][0]!=0.0);
                    if (inc>0.0) burn[j]*=lookup(inc*v[12][0],v[11],(int)(o[12]-o[11]));
                    double cltv=v[17][s]*v[18][s]/v[19][s]*bal[j]/v[1][pm];
                    refi*=spline(cltv,v[7],v[8],(int)(o[8]-o[7]))*v[20][s];
                    double ramp=fmin((v[16][s]+m)/30.0,1.0);
                    double cpr=fmin(pp[4]*ramp*v[5][(int)v[4][m]]*
                                     fmax(1.0+pp[6]*v[2][pm],0.3)*lock+refi,pp[5]);
                    double smm=lookup(cpr*v[10][0],v[9],(int)(o[10]-o[9]));
                    double pmt=bal[j]*r/(1.0-q[j]);
                    q[j]*=1.0+r;
                    double sched=fmin(pmt-bal[j]*r,bal[j]);
                    double prepay=(bal[j]-sched)*smm;
                    intr=bal[j]*(v[14][s]/12.0);
                    principal=sched+prepay;
                    cf=intr+principal;
                    bal[j]-=principal;
                    if (bal[j]<=1e-10) active[j]=false;
                } else {
                    double r=fmax(v[0][pm]+v[6][s],0.0);
                    double attr=v[7][s]*spline(v[12][s]+m,v[4],v[5],(int)(o[5]-o[4]))*v[8][s];
                    attr*=1.0+v[9][s]*logistic(v[10][s]*(v[1][pm]-r-v[11][s]),true);
                    double velocity=v[2][pm];
                    if (velocity>0.0) attr*=1.0+v[14][0]*velocity;
                    attr=fmin(attr,v[15][0]);
                    principal=m==months-1?bal[j]:bal[j]*attr;
                    cf=bal[j]*(r/12.0+v[13][s]/12.0)+principal;
                    intr=bal[j]*r/12.0;
                    bal[j]-=principal;
                    if (bal[j]<=1e-12) active[j]=false;
                }
                cf*=v[3][pm];
            }
            tmp[p]=cf; tmp[paths+p]=intr; tmp[2*paths+p]=principal;
        }
        __syncthreads();
        // Three lanes reduce separate metrics, each in the CPU's path order.
        if (lane<3) {
            double sum=0.0;
            for (int p=0;p<paths;p++) sum+=tmp[lane*paths+p];
            out[(U)lane*span+(U)s*months+m]=sum;
        }
        __syncthreads();
    }
}
